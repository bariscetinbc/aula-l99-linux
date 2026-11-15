import unittest, tempfile, struct, threading, pty, os, select, contextlib, io
from pathlib import Path
from PIL import Image
import panel

class PanelTests(unittest.TestCase):
    def test_images_and_gif(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)/'image.png';Image.new('RGB',(10,20),'red').save(p)
            blob=panel.encode(p,'background')
            self.assertEqual(struct.unpack('<IHHBH',blob[:11]),(307200,320,480,0,panel.crc(blob[11:])))
            self.assertEqual(blob[11:13],b'\x00\xf8')
            p=Path(folder)/'animation.gif'
            Image.new('RGB',(10,20),'red').save(p,save_all=True,append_images=[Image.new('RGB',(10,20),'blue')],duration=100,loop=0)
            blob=panel.encode(p,'gif');self.assertEqual(len(blob)%2048,0)
            self.assertEqual(blob[13],2)
            for i in range(2):
                offset,total,w,h,tag,n,_,delay,checksum=struct.unpack_from('<IIHHBBHHH',blob,20*i)
                self.assertEqual((w,h,tag,n,delay),(320,480,3,2,50))
                self.assertEqual(checksum,panel.crc(blob[40:]))
                self.assertEqual(blob[offset:offset+2],b'LT')
                self.assertEqual(struct.unpack_from('<H',blob,offset+14)[0],2)
                color=struct.unpack_from('<H',blob,offset+16)[0]
                self.assertEqual(color,[0xf800,0x001f][i])
    def test_oversized_image_is_rejected_before_decode(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as folder:
            p = Path(folder) / 'image.png'
            Image.new('RGB', (10, 20)).save(p)
            with patch.object(Image, 'MAX_IMAGE_PIXELS', 150):
                with self.assertRaises(Image.DecompressionBombWarning):
                    panel.encode(p, 'background')

    def test_acknowledged_serial_upload(self):
        master,slave=pty.openpty(); original=panel.panel_port;panel.panel_port=lambda:os.ttyname(slave)
        errors=[];packets=[]
        def device():
            try:
                while True:
                    wire=b''
                    while len(wire)<10:
                        if not select.select([master],[],[],5)[0]: raise TimeoutError('missing packet')
                        wire+=os.read(master,10-len(wire))
                    cmd=wire[4];size=4 if cmd==11 else 11 if cmd==18 else 2048
                    while len(wire)<size+12:wire+=os.read(master,size+12-len(wire))
                    self.assertEqual(wire[:2],b'\x5a\xa5')
                    self.assertEqual(struct.unpack('<H',wire[-2:])[0],panel.crc(wire[:-2],{4:0xeec4,11:0xd141,2048:0xf104}[size]))
                    packets.append(wire)
                    os.write(master,b'\x5a\xa5'+bytes(5)+b'\x5a\xa5\x0b'+bytes(11) if cmd==11 else b'\x5a\xa5'+bytes(15)+b'OK')
                    if cmd==11 and len(packets)==154:break
            except Exception as e:errors.append(e)
        worker=threading.Thread(target=device);worker.start()
        try:
            with tempfile.TemporaryDirectory() as d:
                p=Path(d)/'test.png';Image.new('RGB',(320,480),'blue').save(p)
                with contextlib.redirect_stdout(io.StringIO()):panel.upload(p,'photo-frame')
            worker.join(5);self.assertFalse(worker.is_alive());self.assertFalse(errors);self.assertEqual(len(packets),154)
            self.assertEqual(sum(p[4]==11 for p in packets),3)
        finally:panel.panel_port=original;os.close(master);os.close(slave)

if __name__=='__main__':unittest.main()
