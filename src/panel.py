"""L99 panel encoder/transport. Protocol facts: gavindi/Aula_L99_Linux.
Independent implementation; see README for protocol references and limits.
"""
import os, sys, struct, termios, select, time, warnings
# Reject oversized decompression before allocating full image buffers.
from PIL import Image, ImageSequence
Image.MAX_IMAGE_PIXELS = 16_000_000

def open_image(path):
    with warnings.catch_warnings():
        warnings.simplefilter("error", Image.DecompressionBombWarning)
        return Image.open(path)

def crc(data, value=0xffff):
    for byte in data:
        value ^= byte
        for _ in range(8):
            value = (value >> 1) ^ (0xa001 if value & 1 else 0)
    return value

def rgb565(color):
    r,g,b = color
    return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)

def pixels(image):
    # Pillow 14 removes getdata; retain compatibility with older Pillow releases.
    return image.get_flattened_data() if hasattr(image, "get_flattened_data") else image.getdata()

def encode(path, target):
    with open_image(path) as source:
        if target != 'gif':
            bitmap = source.convert('RGB').resize((320,480))
            data = b''.join(struct.pack('<H',rgb565(p)) for p in pixels(bitmap))
            return struct.pack('<IHHBH',len(data),320,480,0,crc(data)) + data
        if getattr(source,'n_frames',1) > 200:
            raise ValueError('Choose a GIF with at most 200 frames')
        ramp = [(r,g,b) for r in (0,49,99,156,206,255) for g in (0,40,85,130,170,215,255) for b in (0,49,99,156,206,255)]
        palette = Image.new('P',(1,1)); palette.putpalette([c for p in ramp for c in p]+[0]*12)
        frames = []
        for frame in ImageSequence.Iterator(source):
            im = frame.convert('RGB').resize((320,480)).quantize(palette=palette,dither=Image.Dither.FLOYDSTEINBERG).convert('RGB')
            colors = list(dict.fromkeys(pixels(im))); indexes = {p:i for i,p in enumerate(colors)}
            data = bytes(indexes[p] for p in pixels(im))
            header = bytearray(528)
            struct.pack_into('<2sHH2sIHH',header,0,b'LT',320,480,b'\x01\x03',528+len(data),len(data)%65536,2)
            for i,p in enumerate(colors): struct.pack_into('<H',header,16+2*i,rgb565(p))
            frames.append(header+data)
        # Raw bitmap decoders ignore trailing bytes; align to full wire chunks.
        padding = (-(20*len(frames)+sum(map(len,frames))))%2048
        frames[-1] += bytes(padding)
        struct.pack_into('<I',frames[-1],8,len(frames[-1]))
        payload = b''.join(frames); checksum = crc(payload); offset = 20*len(frames); toc = bytearray()
        for frame in frames:
            toc += struct.pack('<IIHHBBHHH',offset,len(payload),320,480,3,len(frames),0,50,checksum)
            offset += len(frame)
        return toc+payload

def packet(cmd, address, payload):
    seed = {2048:0xf104,4:0xeec4,11:0xd141}[len(payload)]
    body = b'\x5a\xa5'+struct.pack('>HBBI',len(payload)//256,cmd,0x66 if cmd==11 else 0x64,address)+payload
    return body+struct.pack('<H',crc(body,seed))

def panel_port():
    from pathlib import Path
    matches=[]
    for node in Path('/sys/class/tty').glob('ttyACM*'):
        p=(node/'device').resolve()
        for parent in [p,*p.parents]:
            try:
                if (parent/'idVendor').read_text().strip()=='eeef' and (parent/'idProduct').read_text().strip()=='268a':
                    matches.append('/dev/'+node.name); break
            except OSError: pass
    if len(matches)!=1: raise ValueError('Connect one L99 touchscreen by USB and scan again')
    return matches[0]

def upload(path,target):
    data=encode(path,target)
    base={'photo-frame':0x041e0000,'background':0x04180000,'gif':0x04240000}[target]
    if len(data)>({'gif':32*1024*1024}.get(target,0x60000)): raise ValueError('Content is too large for the selected destination')
    port=panel_port()
    try:
        fd=os.open(port,os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK)
    except PermissionError:
        raise PermissionError(f"LCD access denied ({port}). Run sh app/install-permissions.sh from the project folder, then unplug and reconnect the keyboard in your local desktop session.") from None
    try:
        attr=termios.tcgetattr(fd); attr[0]=0;attr[1]=0;attr[2]=termios.CS8|termios.CREAD|termios.CLOCAL;attr[3]=0
        attr[4]=attr[5]=termios.B115200;attr[6][termios.VMIN]=0;attr[6][termios.VTIME]=0
        termios.tcsetattr(fd,termios.TCSANOW,attr);termios.tcflush(fd,termios.TCIOFLUSH)
        def send(cmd,address,payload):
            wire=packet(cmd,address,payload); pos=0; deadline=time.monotonic()+8
            while pos<len(wire):
                if time.monotonic()>deadline: raise TimeoutError('Screen write timed out')
                if select.select([],[fd],[],.1)[1]: pos+=os.write(fd,wire[pos:])
            reply=b''; required=21 if cmd==11 else 19
            while len(reply)<required and time.monotonic()<deadline:
                if select.select([fd],[],[],.1)[0]: reply+=os.read(fd,4096)
            valid=reply.startswith(b'\x5a\xa5') and (b'\x5a\xa5' in reply[2:] and 11 in reply[7:10] if cmd==11 else b'OK' in reply)
            if len(reply)<required or not valid: raise ValueError('Screen did not acknowledge upload; reconnect it and retry')
        region=0; count=0
        for offset in range(0,len(data),2048):
            chunk=data[offset:offset+2048];send(7 if len(chunk)==2048 else 18,base+offset,chunk);count+=len(chunk)
            print(f'Uploading {100*(offset+len(chunk))//len(data)}%',flush=True)
            if count==131072 or offset+len(chunk)==len(data):
                send(11,base+region,struct.pack('>I',count));region=offset+len(chunk);count=0
        print('Upload complete. On the LCD select '+('Customized animation' if target=='gif' else 'Photo frame' if target=='photo-frame' else 'Background')+'. Restart the panel if it still shows the old content.',flush=True)
    finally: os.close(fd)

if __name__=='__main__':
    try:
        if sys.argv[1]=='preview':
            open_image(sys.argv[2]).convert('RGBA').resize((160,240)).save(sys.stdout.buffer,format='PNG')
        else: upload(sys.argv[2],sys.argv[1])
    except Exception as error:
        print(str(error),file=sys.stderr);sys.exit(1)
