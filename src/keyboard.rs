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
    pub caps: bool,
    accent: Option<char>,
}

impl Keyboard {
    pub fn label(&self, key: &Key) -> String {
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
        match key.normal {
            "Shift" => {
                self.shift = !self.shift;
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
            _ => {}
        }
        let text = if key.normal == "Space" {
            " ".into()
        } else {
            self.label(key)
        };
        self.shift = false;
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
            && "´`^~¨".contains(c)
        {
            self.accent = Some(c);
            return Action::None;
        }
        Action::Insert(text)
    }
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
