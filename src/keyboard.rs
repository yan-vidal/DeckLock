//! Embedded keyboard model. It never injects input into other applications.
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Insert(String),
    Backspace,
    Submit,
    Close,
    None,
}

#[derive(Clone, Debug)]
pub struct Key {
    pub normal: &'static str,
    pub shifted: &'static str,
    pub width: i32,
}

#[derive(Default)]
pub struct Keyboard {
    pub shift: bool,
    pub shift_locked: bool,
    last_shift_tap: Option<std::time::Instant>,
    pub caps: bool,
    pub altgr: bool,
    pub held_shift: bool,
    pub held_altgr: bool,
    accent: Option<char>,
    pub system_levels: std::collections::HashMap<&'static str, Vec<(String, bool)>>,
}

impl Keyboard {
    fn system_level(&self, key: &Key) -> Option<&(String, bool)> {
        let levels = self.system_levels.get(key.normal)?;
        let alphabetic = levels.first()?.0.chars().all(char::is_alphabetic);
        let shift = self.shift ^ (self.caps && alphabetic);
        let level = usize::from(shift) + if self.altgr { 2 } else { 0 };
        levels.get(level).or_else(|| {
            // Two-level layouts (e.g. plain US) have no AltGr symbols. Use the
            // embedded supplementary layer instead of silently repeating level 0.
            if self.altgr && level_three(key.normal).is_some() {
                None
            } else {
                levels.get(usize::from(shift))
            }
        })
    }
    pub fn label(&self, key: &Key) -> String {
        if let Some((text, _)) = self.system_level(key) {
            return text.clone();
        }
        if self.altgr
            && let Some((normal, shifted, _, _)) = level_three(key.normal)
        {
            return if self.shift { shifted } else { normal }.into();
        }
        if key.normal.chars().count() == 1 && key.normal.chars().all(char::is_alphabetic) {
            if self.shift ^ self.caps {
                key.normal.to_uppercase()
            } else {
                key.normal.into()
            }
        } else if self.shift {
            key.shifted.into()
        } else {
            key.normal.into()
        }
    }

    pub fn press(&mut self, key: &Key) -> Action {
        self.press_at(key, std::time::Instant::now())
    }

    fn press_at(&mut self, key: &Key, now: std::time::Instant) -> Action {
        match key.normal {
            "Shift" => {
                let double = self.last_shift_tap.is_some_and(|last| {
                    now.saturating_duration_since(last) < std::time::Duration::from_millis(600)
                });
                self.last_shift_tap = Some(now);
                if self.shift_locked {
                    self.shift_locked = false;
                    self.shift = self.held_shift || self.shift_locked;
                } else if double && self.shift {
                    self.shift_locked = true;
                } else {
                    self.shift = !self.shift;
                }
                return Action::None;
            }
            "Caps" => {
                self.caps = !self.caps;
                return Action::None;
            }
            "Backspace" => {
                if self.accent.take().is_some() {
                    return Action::None;
                }
                return Action::Backspace;
            }
            "Enter" => return Action::Submit,
            "Close" => return Action::Close,
            "AltGr" => {
                self.altgr = !self.altgr;
                return Action::None;
            }
            _ => {}
        }
        let text = if key.normal == "Space" {
            " ".into()
        } else {
            self.label(key)
        };
        let is_dead = if let Some((_, dead)) = self.system_level(key) {
            *dead
        } else if self.altgr {
            level_three(key.normal).is_some_and(|(_, _, normal_dead, shifted_dead)| {
                if self.shift {
                    shifted_dead
                } else {
                    normal_dead
                }
            })
        } else {
            text.chars().count() == 1 && "´`^~¨".contains(&text)
        };
        self.shift = self.held_shift || self.shift_locked;
        self.altgr = self.held_altgr;
        if let Some(accent) = self.accent.take() {
            if text == " " || text == accent.to_string() {
                return Action::Insert(accent.to_string());
            }
            let mark = match accent {
                '´' => '\u{301}',
                '`' => '\u{300}',
                '^' => '\u{302}',
                '~' => '\u{303}',
                '¨' => '\u{308}',
                '˛' => '\u{328}',
                '¯' => '\u{304}',
                '˝' => '\u{30b}',
                'ˀ' => '\u{309}',
                'ʼ' => '\u{31b}',
                '․' => '\u{323}',
                '˙' => '\u{307}',
                _ => unreachable!(),
            };
            let combined: String = format!("{text}{mark}").nfc().collect();
            return Action::Insert(if combined.chars().count() == 1 {
                combined
            } else {
                format!("{accent}{text}")
            });
        }
        if let Some(c) = text.chars().next().filter(|_| text.chars().count() == 1)
            && is_dead
        {
            self.accent = Some(c);
            return Action::None;
        }
        Action::Insert(text)
    }
}

// Fixed Brazilian ABNT2 levels from the system xkeyboard-config br/latin
// layouts. System-selected XKB groups remain a separate integration concern.
fn level_three(normal: &str) -> Option<(&'static str, &'static str, bool, bool)> {
    let (a, b, dead_a, dead_b) = match normal {
        "1" => ("¹", "¡", false, false),
        "2" => ("²", "½", false, false),
        "3" => ("³", "¾", false, false),
        "4" => ("£", "¼", false, false),
        "5" => ("¢", "⅜", false, false),
        "6" => ("¬", "¨", false, false),
        "7" => ("{", "⅞", false, false),
        "8" => ("[", "™", false, false),
        "9" => ("]", "±", false, false),
        "0" => ("}", "°", false, false),
        "-" => ("\\", "¿", false, false),
        "=" => ("§", "˛", false, true),
        "q" => ("/", "/", false, false),
        "w" => ("?", "?", false, false),
        "e" => ("°", "°", false, false),
        "r" => ("®", "®", false, false),
        "t" => ("ŧ", "Ŧ", false, false),
        "y" => ("←", "¥", false, false),
        "u" => ("↓", "↑", false, false),
        "i" => ("→", "ı", false, false),
        "o" => ("ø", "Ø", false, false),
        "p" => ("þ", "Þ", false, false),
        "´" => ("´", "`", false, false),
        "[" => ("ª", "¯", false, true),
        "]" => ("º", "º", false, false),
        "a" => ("æ", "Æ", false, false),
        "s" => ("ß", "ẞ", false, false),
        "d" => ("ð", "Ð", false, false),
        "f" => ("đ", "ª", false, false),
        "g" => ("ŋ", "Ŋ", false, false),
        "h" => ("ħ", "Ħ", false, false),
        "j" => ("ˀ", "ʼ", true, true),
        "k" => ("ĸ", "&", false, false),
        "l" => ("ł", "Ł", false, false),
        "ç" => ("´", "˝", true, true),
        "~" => ("~", "^", false, false),
        "'" => ("¬", "¬", false, false),
        "z" => ("«", "<", false, false),
        "x" => ("»", ">", false, false),
        "c" => ("©", "©", false, false),
        "v" => ("„", "‚", false, false),
        "b" => ("“", "‘", false, false),
        "n" => ("”", "’", false, false),
        "m" => ("µ", "µ", false, false),
        "," => ("•", "×", false, false),
        "." => ("·", "÷", false, false),
        ";" => ("․", "˙", true, true),
        _ => return None,
    };
    Some((a, b, dead_a, dead_b))
}

pub fn rows() -> Vec<Vec<Key>> {
    let row = |normal: &'static str, shifted: &'static str| {
        normal
            .split_whitespace()
            .zip(shifted.split_whitespace())
            .map(|(normal, shifted)| Key {
                normal,
                shifted,
                width: 1,
            })
            .collect::<Vec<_>>()
    };
    let mut rows = vec![
        row("1 2 3 4 5 6 7 8 9 0 - =", "! @ # $ % ¨ & * ( ) _ +"),
        row("q w e r t y u i o p ´ [", "Q W E R T Y U I O P ` {"),
        row("a s d f g h j k l ç ~ ]", "A S D F G H J K L Ç ^ }"),
        row(r"z x c v b n m , . ; / \", "Z X C V B N M < > : ? |"),
    ];
    rows.push(vec![
        Key {
            normal: "Shift",
            shifted: "Shift",
            width: 2,
        },
        Key {
            normal: "Caps",
            shifted: "Caps",
            width: 1,
        },
        Key {
            normal: "'",
            shifted: "\"",
            width: 1,
        },
        Key {
            normal: "Space",
            shifted: "Space",
            width: 4,
        },
        Key {
            normal: "Backspace",
            shifted: "Backspace",
            width: 2,
        },
        Key {
            normal: "Enter",
            shifted: "Enter",
            width: 2,
        },
    ]);
    rows
}

/// Coordinates from layout/keyboard.svg, in its 800 x 405 viewbox.
/// Kept separate from input logic so mouse hit boxes and pad calibration agree.
pub fn geometry() -> Vec<(Key, f64, f64, f64, f64)> {
    vec![
        (
            Key {
                normal: "´",
                shifted: "`",
                width: 1,
            },
            159.99997,
            331.42599,
            68.00000,
            67.00000,
        ),
        (
            Key {
                normal: "[",
                shifted: "{",
                width: 1,
            },
            231.99997,
            331.42599,
            68.00000,
            67.00000,
        ),
        (
            Key {
                normal: "~",
                shifted: "^",
                width: 1,
            },
            588.99997,
            331.42599,
            62.00000,
            67.00000,
        ),
        (
            Key {
                normal: "=",
                shifted: "+",
                width: 1,
            },
            654.99997,
            331.42599,
            62.00000,
            67.00000,
        ),
        (
            Key {
                normal: "Space",
                shifted: "Space",
                width: 1,
            },
            307.99997,
            331.42581,
            183.00000,
            67.00000,
        ),
        (
            Key {
                normal: "Shift",
                shifted: "Shift",
                width: 1,
            },
            7.99997,
            331.42581,
            148.00000,
            67.00000,
        ),
        (
            Key {
                normal: "AltGr",
                shifted: "AltGr",
                width: 1,
            },
            720.99997,
            331.42581,
            70.00000,
            67.00000,
        ),
        (
            Key {
                normal: "Backspace",
                shifted: "Backspace",
                width: 1,
            },
            494.99997,
            331.42581,
            90.00000,
            67.00000,
        ),
        (
            Key {
                normal: "'",
                shifted: "\"",
                width: 1,
            },
            6.62497,
            241.42599,
            59.00000,
            81.00000,
        ),
        (
            Key {
                normal: "z",
                shifted: "Z",
                width: 1,
            },
            69.62497,
            241.42599,
            68.00000,
            81.00000,
        ),
        (
            Key {
                normal: "x",
                shifted: "X",
                width: 1,
            },
            141.62497,
            241.42599,
            68.00000,
            81.00000,
        ),
        (
            Key {
                normal: "c",
                shifted: "C",
                width: 1,
            },
            213.62497,
            241.42599,
            68.00000,
            81.00000,
        ),
        (
            Key {
                normal: "v",
                shifted: "V",
                width: 1,
            },
            285.62497,
            241.42599,
            70.00000,
            81.00000,
        ),
        (
            Key {
                normal: "b",
                shifted: "B",
                width: 1,
            },
            359.62497,
            241.42599,
            80.00000,
            81.00000,
        ),
        (
            Key {
                normal: "n",
                shifted: "N",
                width: 1,
            },
            443.62497,
            241.42599,
            70.00000,
            81.00000,
        ),
        (
            Key {
                normal: "m",
                shifted: "M",
                width: 1,
            },
            517.62497,
            241.42599,
            68.00000,
            81.00000,
        ),
        (
            Key {
                normal: ",",
                shifted: "<",
                width: 1,
            },
            589.62497,
            241.42599,
            64.60000,
            81.00000,
        ),
        (
            Key {
                normal: ".",
                shifted: ">",
                width: 1,
            },
            658.22497,
            241.42599,
            64.60000,
            81.00000,
        ),
        (
            Key {
                normal: ";",
                shifted: ":",
                width: 1,
            },
            726.82497,
            241.42599,
            64.60000,
            81.00000,
        ),
        (
            Key {
                normal: "-",
                shifted: "_",
                width: 1,
            },
            6.62497,
            166.42599,
            45.00000,
            67.00000,
        ),
        (
            Key {
                normal: "a",
                shifted: "A",
                width: 1,
            },
            55.62497,
            166.42599,
            75.00000,
            67.00000,
        ),
        (
            Key {
                normal: "s",
                shifted: "S",
                width: 1,
            },
            134.62497,
            166.42599,
            68.00000,
            67.00000,
        ),
        (
            Key {
                normal: "d",
                shifted: "D",
                width: 1,
            },
            206.62497,
            166.42599,
            62.00000,
            67.00000,
        ),
        (
            Key {
                normal: "f",
                shifted: "F",
                width: 1,
            },
            272.62497,
            166.42599,
            62.00000,
            67.00000,
        ),
        (
            Key {
                normal: "g",
                shifted: "G",
                width: 1,
            },
            338.62497,
            166.42599,
            60.50000,
            67.00000,
        ),
        (
            Key {
                normal: "h",
                shifted: "H",
                width: 1,
            },
            403.12497,
            166.42599,
            60.50000,
            67.00000,
        ),
        (
            Key {
                normal: "j",
                shifted: "J",
                width: 1,
            },
            467.62497,
            166.42599,
            62.00000,
            67.00000,
        ),
        (
            Key {
                normal: "k",
                shifted: "K",
                width: 1,
            },
            533.62497,
            166.42599,
            68.00000,
            67.00000,
        ),
        (
            Key {
                normal: "l",
                shifted: "L",
                width: 1,
            },
            605.62497,
            166.42599,
            63.75736,
            67.00000,
        ),
        (
            Key {
                normal: "Enter",
                shifted: "Enter",
                width: 1,
            },
            731.06638,
            166.42599,
            60.55861,
            67.00000,
        ),
        (
            Key {
                normal: "ç",
                shifted: "Ç",
                width: 1,
            },
            673.38233,
            166.42599,
            53.85876,
            67.00000,
        ),
        (
            Key {
                normal: "q",
                shifted: "Q",
                width: 1,
            },
            6.62497,
            77.42599,
            75.00000,
            82.00000,
        ),
        (
            Key {
                normal: "w",
                shifted: "W",
                width: 1,
            },
            85.62497,
            77.42599,
            72.00000,
            82.00000,
        ),
        (
            Key {
                normal: "e",
                shifted: "E",
                width: 1,
            },
            161.62497,
            77.42599,
            62.00000,
            82.00000,
        ),
        (
            Key {
                normal: "r",
                shifted: "R",
                width: 1,
            },
            227.62497,
            77.42599,
            62.00000,
            82.00000,
        ),
        (
            Key {
                normal: "t",
                shifted: "T",
                width: 1,
            },
            293.62497,
            77.42599,
            62.00000,
            82.00000,
        ),
        (
            Key {
                normal: "y",
                shifted: "Y",
                width: 1,
            },
            359.62497,
            77.42599,
            80.00000,
            82.00000,
        ),
        (
            Key {
                normal: "u",
                shifted: "U",
                width: 1,
            },
            443.62497,
            77.42599,
            62.00000,
            82.00000,
        ),
        (
            Key {
                normal: "i",
                shifted: "I",
                width: 1,
            },
            509.62497,
            77.42599,
            62.00000,
            82.00000,
        ),
        (
            Key {
                normal: "o",
                shifted: "O",
                width: 1,
            },
            575.62497,
            77.42599,
            72.00000,
            82.00000,
        ),
        (
            Key {
                normal: "p",
                shifted: "P",
                width: 1,
            },
            651.62497,
            77.42599,
            75.00000,
            82.00000,
        ),
        (
            Key {
                normal: "]",
                shifted: "}",
                width: 1,
            },
            730.62497,
            77.42599,
            61.00000,
            82.00000,
        ),
        (
            Key {
                normal: "1",
                shifted: "!",
                width: 1,
            },
            6.62497,
            6.42599,
            68.00000,
            64.50000,
        ),
        (
            Key {
                normal: "2",
                shifted: "@",
                width: 1,
            },
            78.62497,
            6.42599,
            68.00000,
            64.50000,
        ),
        (
            Key {
                normal: "3",
                shifted: "#",
                width: 1,
            },
            150.62497,
            6.42599,
            68.00000,
            64.50000,
        ),
        (
            Key {
                normal: "4",
                shifted: "$",
                width: 1,
            },
            222.62497,
            6.42599,
            68.00000,
            64.50000,
        ),
        (
            Key {
                normal: "5",
                shifted: "%",
                width: 1,
            },
            294.62497,
            6.42599,
            104.00000,
            64.50000,
        ),
        (
            Key {
                normal: "6",
                shifted: "¨",
                width: 1,
            },
            403.12497,
            6.42599,
            102.50781,
            64.50000,
        ),
        (
            Key {
                normal: "7",
                shifted: "&",
                width: 1,
            },
            509.65790,
            6.42599,
            65.96707,
            64.50000,
        ),
        (
            Key {
                normal: "8",
                shifted: "*",
                width: 1,
            },
            579.62497,
            6.42599,
            68.00000,
            64.50000,
        ),
        (
            Key {
                normal: "9",
                shifted: "(",
                width: 1,
            },
            651.62497,
            6.42599,
            68.00000,
            64.50000,
        ),
        (
            Key {
                normal: "0",
                shifted: ")",
                width: 1,
            },
            723.62497,
            6.42599,
            68.00000,
            64.50000,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(normal: &'static str, shifted: &'static str) -> Key {
        Key {
            normal,
            shifted,
            width: 1,
        }
    }
    #[test]
    fn alt_uses_supplementary_symbols_when_system_has_only_two_levels() {
        let mut model = Keyboard::default();
        model
            .system_levels
            .insert("1", vec![("1".into(), false), ("!".into(), false)]);
        let key = Key {
            normal: "1",
            shifted: "!",
            width: 1,
        };
        model.altgr = true;
        assert_eq!(model.label(&key), "¹");
        assert_eq!(model.press(&key), Action::Insert("¹".into()));
        model.system_levels.insert(
            "1",
            vec![
                ("1".into(), false),
                ("!".into(), false),
                ("custom".into(), false),
            ],
        );
        model.altgr = true;
        assert_eq!(model.label(&key), "custom");
    }

    #[test]
    fn double_shift_latches_until_next_shift_and_slow_taps_do_not() {
        let mut model = Keyboard::default();
        let shift = Key {
            normal: "Shift",
            shifted: "Shift",
            width: 1,
        };
        let letter = Key {
            normal: "a",
            shifted: "A",
            width: 1,
        };
        let start = std::time::Instant::now();
        model.press_at(&shift, start);
        model.press_at(&shift, start + std::time::Duration::from_millis(200));
        assert_eq!(model.press(&letter), Action::Insert("A".into()));
        assert_eq!(model.press(&letter), Action::Insert("A".into()));
        model.press_at(&shift, start + std::time::Duration::from_millis(300));
        assert_eq!(model.press(&letter), Action::Insert("a".into()));
        model.press_at(&shift, start + std::time::Duration::from_secs(2));
        model.press_at(&shift, start + std::time::Duration::from_secs(3));
        assert_eq!(model.press(&letter), Action::Insert("a".into()));
    }

    #[test]
    fn system_keymap_preserves_literal_symbols_and_dead_keys() {
        let mut model = Keyboard::default();
        let key = Key {
            normal: "~",
            shifted: "^",
            width: 1,
        };
        model
            .system_levels
            .insert("~", vec![("'".into(), false), ("\"".into(), false)]);
        assert_eq!(model.press(&key), Action::Insert("'".into()));
        model.shift = true;
        assert_eq!(model.press(&key), Action::Insert("\"".into()));
        model.system_levels.insert("~", vec![("´".into(), true)]);
        assert_eq!(model.press(&key), Action::None);
        assert_eq!(
            model.press(&Key {
                normal: "e",
                shifted: "E",
                width: 1
            }),
            Action::Insert("é".into())
        );
    }

    #[test]
    fn svg_geometry_keeps_original_rows_and_separate_pad_halves() {
        let keys = geometry();
        assert_eq!(keys.len(), 52);
        assert!(
            keys.iter()
                .all(|(_, x, y, w, h)| *x >= 0.0 && *y >= 0.0 && x + w <= 800.0 && y + h <= 405.0)
        );
        let (_, x, y, _, _) = keys.iter().find(|(key, ..)| key.normal == "Enter").unwrap();
        assert!(*x > 700.0 && *y > 160.0 && *y < 170.0);
        let (_, _, y, width, _) = keys.iter().find(|(key, ..)| key.normal == "Space").unwrap();
        assert!(*y > 330.0 && *width > 180.0);
    }
    #[test]
    fn altgr_makes_symbols_accessible_and_distinguishes_literal_accents() {
        let mut model = Keyboard::default();
        model.press(&key("AltGr", "AltGr"));
        assert_eq!(model.label(&key("q", "Q")), "/");
        assert_eq!(model.press(&key("q", "Q")), Action::Insert("/".into()));
        assert!(!model.altgr);
        model.press(&key("AltGr", "AltGr"));
        assert_eq!(model.press(&key("´", "`")), Action::Insert("´".into()));
        model.press(&key("AltGr", "AltGr"));
        assert_eq!(model.press(&key("ç", "Ç")), Action::None);
        assert_eq!(model.press(&key("a", "A")), Action::Insert("á".into()));
    }
    #[test]
    fn shift_is_one_shot_and_caps_inverts_with_shift() {
        let mut k = Keyboard::default();
        k.press(&key("Shift", "Shift"));
        assert_eq!(k.press(&key("a", "A")), Action::Insert("A".into()));
        assert_eq!(k.press(&key("a", "A")), Action::Insert("a".into()));
        k.press(&key("Caps", "Caps"));
        k.press(&key("Shift", "Shift"));
        assert_eq!(k.press(&key("a", "A")), Action::Insert("a".into()));
    }
    #[test]
    fn accents_compose_or_preserve_both_characters() {
        let mut k = Keyboard::default();
        assert_eq!(k.press(&key("´", "`")), Action::None);
        assert_eq!(k.press(&key("e", "E")), Action::Insert("é".into()));
        k.press(&key("~", "^"));
        assert_eq!(k.press(&key("Space", "Space")), Action::Insert("~".into()));
        k.press(&key("´", "`"));
        assert_eq!(k.press(&key("1", "!")), Action::Insert("´1".into()));
    }
    #[test]
    fn backspace_cancels_pending_accent_without_deleting_password() {
        let mut k = Keyboard::default();
        k.press(&key("´", "`"));
        assert_eq!(k.press(&key("Backspace", "Backspace")), Action::None);
        assert_eq!(k.press(&key("Backspace", "Backspace")), Action::Backspace);
    }
}
