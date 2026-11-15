"""L99-only evdev → uinput remapper. Started explicitly through PolicyKit.
No dependencies; EOF on the parent pipe releases the physical keyboard.
"""
import array
import fcntl
import glob
import json
import os
import select
import struct
import sys

EVENT = struct.Struct('@llHHi')

class Mapper:
    def __init__(self, mappings):
        self.ctrl_alt_altgr = isinstance(mappings, dict) and mappings.get('ctrl_alt_altgr', False)
        self.shift2_at = isinstance(mappings, dict) and mappings.get('shift2_at', False)
        self.at_key_down = False
        entries = mappings.get('mappings', []) if isinstance(mappings, dict) else mappings
        if type(self.shift2_at) is not bool or type(self.ctrl_alt_altgr) is not bool or not isinstance(entries, list) or len(entries) > 84:
            raise ValueError('Invalid keyboard mappings')
        checked = {}
        for entry in entries:
            if not isinstance(entry, (list, tuple)) or len(entry) != 2:
                raise ValueError('Invalid keyboard mapping entry')
            key, outputs = entry
            if type(key) is not int or not 1 <= key < 768 or not isinstance(outputs, list) or len(outputs) > 2:
                raise ValueError('Invalid keyboard mapping keys')
            if any(type(code) is not int or not 1 <= code < 768 for code in outputs) or len(set(outputs)) != len(outputs):
                raise ValueError('Invalid keyboard mapping outputs')
            checked[key] = outputs
        self.mappings = checked
        self.physical = set()
        self.chord = False
        self.held = {}
        self.counts = {}

    def event(self, code, value):
        if code == 3 and (self.at_key_down or (
                self.shift2_at and value == 1 and
                any(self.counts.get(k, 0) for k in (42, 54)) and
                not any(self.counts.get(k, 0) for k in (29, 97, 56, 100, 125, 126)))):
            self.at_key_down = value != 0
            if not value:
                return []
            # Emit one complete character, never leave AltGr active for Enter
            # or another key. Preserve both Shift keys and existing Q ownership.
            shifts = [k for k in (42, 54) if self.counts.get(k, 0)]
            q_held = bool(self.counts.get(16, 0))
            return ([(k, 0) for k in shifts] +
                    ([(16, 0)] if q_held else []) +
                    [(100, 1), (16, 1), (16, 0), (100, 0)] +
                    ([(16, 1)] if q_held else []) +
                    [(k, 1) for k in shifts])
        if not self.ctrl_alt_altgr:
            return self._event(code, value)
        if value == 2:
            return self._event(code, value)
        if value:
            self.physical.add(code)
        else:
            self.physical.discard(code)
        # Only the documented @ shortcut needs AltGr. Converting the whole
        # modifier pair swallows Ctrl+Alt shortcuts and undefined layout levels.
        chord = {29, 56, 16}.issubset(self.physical)
        if chord != self.chord:
            result = []
            # Release the previous modifier outputs before replacing the pair.
            for key in (29, 56):
                result += self._event(key, 0)
            self.chord = chord
            for key in (29, 56):
                if key in self.physical:
                    result += self._event(key, 1)
            # Q itself must still receive its press/release when it changes
            # the chord; otherwise it can be swallowed or left held forever.
            if code not in (29, 56):
                if not value:
                    # Release the character before restoring normal modifiers.
                    return self._event(code, value) + result
                result += self._event(code, value)
            return result
        return self._event(code, value)

    def _event(self, code, value):
        if value == 2:
            return [(k, 2) for k in self.held.get(code, [code]) if k not in (42, 54, 29, 97, 56, 100)]
        if value:
            if code in self.held:
                return []
            keys = ([100] if self.chord else [code]) if self.ctrl_alt_altgr and code in (29, 56) else self.mappings.get(code, [code])
            self.held[code] = keys
            result = []
            for k in keys:
                self.counts[k] = self.counts.get(k, 0) + 1
                if self.counts[k] == 1:
                    result.append((k, 1))
            return result
        result = []
        for k in reversed(self.held.pop(code, [])):
            self.counts[k] -= 1
            if self.counts[k] == 0:
                result.append((k, 0))
        return result


def run(mappings):
    Mapper(mappings)  # Validate before opening or grabbing any privileged devices.
    devices = []
    virtual = None
    try:
        # Match the wired L99, never other physical keyboards or our virtual one.
        for path in glob.glob('/sys/class/input/event*'):
            def read(name):
                with open(path + '/device/id/' + name) as f:
                    return f.read().strip().lower()
            if read('vendor') != '0c45' or read('product') != '800a':
                continue
            fd = os.open('/dev/input/' + os.path.basename(path), os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
            identity = array.array('H', [0] * 4)
            fcntl.ioctl(fd, 0x80084502, identity, True)  # EVIOCGID
            if identity[1:3].tolist() != [0x0c45, 0x800a]:
                os.close(fd)
                continue
            bits = array.array('B', [0] * 96)
            fcntl.ioctl(fd, 0x80604521, bits, True)  # EVIOCGBIT(EV_KEY, 96)
            if not bits[30 // 8] & (1 << (30 % 8)):  # KEY_A: skip consumer/mouse interfaces
                os.close(fd)
                continue
            devices.append(fd)
        if not devices:
            raise RuntimeError('Wired L99 keyboard not found. Connect USB and try again.')
        virtual = os.open('/dev/uinput', os.O_WRONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
        fcntl.ioctl(virtual, 0x40045564, 1)  # UI_SET_EVBIT EV_KEY
        for code in range(1, 768):
            fcntl.ioctl(virtual, 0x40045565, code)
        descriptor = struct.pack('80sHHHHI', b'AULA L99 mapped keyboard', 3, 0x1209, 0x0099, 1, 0) + bytes(1024)
        os.write(virtual, descriptor)
        fcntl.ioctl(virtual, 0x5501)  # UI_DEV_CREATE
        for fd in devices:
            fcntl.ioctl(fd, 0x40044590, 1)  # EVIOCGRAB
        mappers = {fd: Mapper(mappings) for fd in devices}
        print('Mapping active · keep this app open; Stop restores normal keys', flush=True)
        while True:
            ready, _, _ = select.select([0] + devices, [], [], 1)
            if 0 in ready and not os.read(0, 1):
                break
            for fd in devices:
                if fd not in ready:
                    continue
                data = os.read(fd, EVENT.size * 64)
                if not data:
                    raise RuntimeError('Keyboard disconnected; mappings stopped')
                for offset in range(0, len(data), EVENT.size):
                    _, _, kind, code, value = EVENT.unpack_from(data, offset)
                    if kind == 1:
                        for out_code, out_value in mappers[fd].event(code, value):
                            os.write(virtual, EVENT.pack(0, 0, 1, out_code, out_value))
                        os.write(virtual, EVENT.pack(0, 0, 0, 0, 0))
    finally:
        for fd in devices:
            try:
                fcntl.ioctl(fd, 0x40044590, 0)
            finally:
                os.close(fd)
        if virtual is not None:
            try:
                fcntl.ioctl(virtual, 0x5502)
            finally:
                os.close(virtual)

if __name__ == '__main__':
    try:
        run(json.loads(sys.argv[1]))
    except Exception as error:
        print('Mapping stopped: ' + str(error), flush=True)
        sys.exit(1)
