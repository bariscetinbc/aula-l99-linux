import unittest
from remap import Mapper

class MappingTests(unittest.TestCase):
    def test_shift2_restores_modifiers_and_enter(self):
        for shifts in ((42,), (54,), (42, 54)):
            for release_shift_first in (False, True):
                m = Mapper({'shift2_at': True})
                for key in shifts:
                    m.event(key, 1)
                for value in (1, 2):
                    output = m.event(3, value)
                    self.assertIn((16, 1), output)
                    self.assertIn((100, 0), output)
                    self.assertEqual(output[-len(shifts):], [(k, 1) for k in shifts])
                    self.assertEqual(m.event(28, 1), [(28, 1)])
                    self.assertEqual(m.event(28, 0), [(28, 0)])
                if release_shift_first:
                    for key in shifts:
                        m.event(key, 0)
                self.assertEqual(m.event(3, 0), [])
                if not release_shift_first:
                    for key in shifts:
                        m.event(key, 0)
                self.assertFalse(any(m.counts.values()))
                self.assertEqual(m.event(28, 1), [(28, 1)])

    def test_shift2_preserves_other_shortcuts(self):
        m = Mapper({'shift2_at': True})
        for code, value in [(3, 1), (3, 0), (29, 1), (42, 1), (3, 1), (3, 0), (42, 0), (29, 0)]:
            self.assertEqual(m.event(code, value), [(code, value)])

    def test_ctrl_alt_q_both_orders_and_releases(self):
        for first, second in [(29, 56), (56, 29)]:
            for released in (29, 56, 16):
                with self.subTest(first=first, released=released):
                    m = Mapper({'ctrl_alt_altgr': True, 'mappings': []})
                    self.assertEqual(m.event(first, 1), [(first, 1)])
                    self.assertEqual(m.event(second, 1), [(second, 1)])
                    self.assertEqual(m.event(16, 1), [(29, 0), (56, 0), (100, 1), (16, 1)])
                    self.assertEqual(m.event(16, 2), [(16, 2)])
                    events = m.event(released, 0)
                    self.assertIn((100, 0), events)
                    for key in (29, 56, 16):
                        if key != released:
                            m.event(key, 0)
                    self.assertFalse(any(m.counts.values()))
                    self.assertFalse(m.held)

    def test_other_ctrl_alt_shortcuts_pass_through(self):
        for key in (14, 30, 46, 59, 111):
            m = Mapper({'ctrl_alt_altgr': True})
            for code, value in [(29, 1), (56, 1), (key, 1), (key, 2),
                                (key, 0), (29, 0), (56, 0), (30, 1), (30, 0)]:
                self.assertEqual(m.event(code, value), [(code, value)])
            self.assertFalse(any(m.counts.values()))

    def test_real_altgr_is_not_released_by_chord(self):
        m = Mapper({'ctrl_alt_altgr': True})
        self.assertEqual(m.event(100, 1), [(100, 1)])
        m.event(29, 1)
        m.event(56, 1)
        self.assertEqual(m.event(16, 1), [(29, 0), (56, 0), (16, 1)])
        self.assertEqual(m.event(16, 0), [(16, 0), (29, 1), (56, 1)])
        m.event(29, 0)
        m.event(56, 0)
        self.assertEqual(m.event(100, 0), [(100, 0)])

    def test_altgr_and_at_chord(self):
        m = Mapper([[97, [100]], [16, [100, 16]]])
        self.assertEqual(m.event(97, 1), [(100, 1)])
        self.assertEqual(m.event(16, 1), [(16, 1)])
        self.assertEqual(m.event(16, 0), [(16, 0)])
        self.assertEqual(m.event(97, 0), [(100, 0)])

    def test_repeat_and_release_order(self):
        m = Mapper([[16, [42, 3]]])
        self.assertEqual(m.event(16, 1), [(42, 1), (3, 1)])
        self.assertEqual(m.event(16, 2), [(3, 2)])
        self.assertEqual(m.event(16, 0), [(3, 0), (42, 0)])
        self.assertEqual(m.event(16, 0), [])

    def test_invalid_privileged_input_is_rejected(self):
        for payload in ([[30, [0]]], [[30, [768]]], [[30, [42, 42]]], [[True, [16]]], [[30, [1, 2, 3]]], {'mappings': [], 'ctrl_alt_altgr': 'yes'}, {'shift2_at': 'yes'}):
            with self.subTest(payload=payload), self.assertRaises(ValueError):
                Mapper(payload)

    def test_disabled_and_unmapped(self):
        m = Mapper([[30, []]])
        self.assertEqual(m.event(30, 1), [])
        self.assertEqual(m.event(30, 2), [])
        self.assertEqual(m.event(30, 0), [])
        self.assertEqual(m.event(31, 1), [(31, 1)])
        self.assertEqual(m.event(31, 0), [(31, 0)])

if __name__ == '__main__':
    unittest.main()
