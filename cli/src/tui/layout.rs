//! Responsive layout: width breakpoints, column planning for tables and text
//! helpers (ellipsis, word wrap) that respect the terminal cell width.

use std::cmp::Reverse;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Width breakpoints: a phone in portrait is narrow, a laptop terminal wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// Fewer than 80 columns.
    Narrow,
    /// 80 to 119 columns.
    Medium,
    /// 120 columns or more.
    Wide,
}

impl Size {
    pub fn of(width: u16) -> Self {
        match width {
            0..=79 => Self::Narrow,
            80..=119 => Self::Medium,
            _ => Self::Wide,
        }
    }

    pub fn narrow(self) -> bool {
        self == Self::Narrow
    }
}

/// How wide a table column is.
#[derive(Debug, Clone, Copy)]
pub enum Width {
    Fixed(u16),
    /// At least `min` and at most `max` cells; leftover space is shared by `weight`.
    Flex {
        min: u16,
        weight: u16,
        max: u16,
    },
}

impl Width {
    /// Grows without limit.
    pub const fn flex(min: u16, weight: u16) -> Self {
        Self::Flex {
            min,
            weight,
            max: u16::MAX,
        }
    }

    /// Sized to its content: `longest` cells plus one, but at least `min` and at most `cap`.
    /// The weight is high so that it reaches its size before sparklines and other
    /// growing columns take the spare room.
    pub fn content(min: u16, longest: usize, cap: u16) -> Self {
        // Names up to 16 cells are never squeezed: other columns go first.
        let wanted = u16::try_from(longest + 1).unwrap_or(u16::MAX);
        let min = min.max(wanted.min(16)).min(cap);
        let max = wanted.clamp(min, cap);
        Self::Flex {
            min,
            weight: 16,
            max,
        }
    }
}

/// A table column and how much it matters: low priorities are dropped first.
#[derive(Debug, Clone, Copy)]
pub struct Col<T> {
    pub id: T,
    pub header: &'static str,
    pub width: Width,
    pub priority: u8,
}

/// A column that made it into the layout, with its final width.
#[derive(Debug, Clone, Copy)]
pub struct Planned<T> {
    pub id: T,
    pub header: &'static str,
    pub width: u16,
}

/// Chooses the columns that fit into `avail` cells (one cell of spacing between
/// columns), dropping the least important ones first, and sizes them.
pub fn plan<T: Copy>(cols: &[Col<T>], avail: u16) -> Vec<Planned<T>> {
    let base = |c: &Col<T>| match c.width {
        Width::Fixed(n) => n,
        Width::Flex { min, .. } => min,
    };
    let need = |kept: &[Col<T>]| -> u16 {
        kept.iter().map(base).sum::<u16>() + kept.len().saturating_sub(1) as u16
    };
    let mut kept: Vec<Col<T>> = cols.to_vec();
    while kept.len() > 1 && need(&kept) > avail {
        // lowest priority goes; among equals the right-most
        if let Some((index, _)) = kept
            .iter()
            .enumerate()
            .min_by_key(|(i, c)| (c.priority, Reverse(*i)))
        {
            kept.remove(index);
        }
    }
    let mut widths: Vec<u16> = kept.iter().map(base).collect();
    let mut left = avail.saturating_sub(need(&kept));
    let mut active: Vec<usize> = kept
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.width, Width::Flex { .. }))
        .map(|(i, _)| i)
        .collect();
    while left > 0 && !active.is_empty() {
        let total: u32 = active.iter().map(|&i| flex_of(&kept[i]).0).sum();
        let mut given = 0;
        for &i in &active {
            let (weight, max) = flex_of(&kept[i]);
            let share = (u32::from(left) * weight / total) as u16;
            let grant = share.min(max - widths[i]);
            widths[i] += grant;
            given += grant;
        }
        if given == 0 {
            for &i in &active {
                if left > given && widths[i] < flex_of(&kept[i]).1 {
                    widths[i] += 1;
                    given += 1;
                }
            }
        }
        left -= given;
        active.retain(|&i| widths[i] < flex_of(&kept[i]).1);
        if given == 0 {
            break;
        }
    }
    let mut planned: Vec<Planned<T>> = kept
        .iter()
        .zip(widths)
        .map(|(c, width)| Planned {
            id: c.id,
            header: c.header,
            width,
        })
        .collect();
    if planned.len() == 1 {
        planned[0].width = planned[0].width.min(avail.max(1));
    }
    planned
}

fn flex_of<T>(col: &Col<T>) -> (u32, u16) {
    match col.width {
        Width::Flex { weight, max, .. } => (u32::from(weight.max(1)), max),
        Width::Fixed(n) => (0, n),
    }
}

/// `text` cut to `width` cells, with `…` where it was shortened.
pub fn ellipsize(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// Word-wraps `text` into lines of at most `width` cells; long words are cut.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let word = if word.width() > width {
            ellipsize(word, width)
        } else {
            word.to_owned()
        };
        let joined = if line.is_empty() {
            word.width()
        } else {
            line.width() + 1 + word.width()
        };
        if joined > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Id {
        A,
        B,
        C,
        D,
    }

    fn cols() -> Vec<Col<Id>> {
        vec![
            Col {
                id: Id::A,
                header: "A",
                width: Width::Fixed(10),
                priority: 9,
            },
            Col {
                id: Id::B,
                header: "B",
                width: Width::flex(8, 1),
                priority: 10,
            },
            Col {
                id: Id::C,
                header: "C",
                width: Width::Fixed(6),
                priority: 2,
            },
            Col {
                id: Id::D,
                header: "D",
                width: Width::Fixed(8),
                priority: 5,
            },
        ]
    }

    fn ids(planned: &[Planned<Id>]) -> Vec<Id> {
        planned.iter().map(|p| p.id).collect()
    }

    #[test]
    fn breakpoints() {
        assert_eq!(Size::of(32), Size::Narrow);
        assert_eq!(Size::of(79), Size::Narrow);
        assert_eq!(Size::of(80), Size::Medium);
        assert_eq!(Size::of(119), Size::Medium);
        assert_eq!(Size::of(120), Size::Wide);
    }

    #[test]
    fn plan_keeps_everything_when_it_fits_and_gives_the_rest_to_flex() {
        let planned = plan(&cols(), 60);
        assert_eq!(ids(&planned), [Id::A, Id::B, Id::C, Id::D]);
        let total: u16 = planned.iter().map(|p| p.width).sum::<u16>() + 3;
        assert_eq!(total, 60, "the columns fill the width exactly");
        assert_eq!(planned[1].width, 60 - 3 - 10 - 6 - 8);
    }

    #[test]
    fn content_columns_stop_growing_and_pass_the_rest_on() {
        let cols = [
            Col {
                id: Id::A,
                header: "A",
                width: Width::content(4, 5, 20),
                priority: 9,
            },
            Col {
                id: Id::B,
                header: "B",
                width: Width::flex(8, 1),
                priority: 10,
            },
            Col {
                id: Id::C,
                header: "C",
                width: Width::Flex {
                    min: 6,
                    weight: 2,
                    max: 12,
                },
                priority: 5,
            },
        ];
        let planned = plan(&cols, 80);
        assert_eq!(
            planned[0].width, 6,
            "content width: the longest value plus one"
        );
        assert_eq!(planned[2].width, 12, "capped");
        assert_eq!(
            planned[1].width,
            80 - 2 - 6 - 12,
            "the uncapped column takes the rest"
        );
        // a cap below the minimum wins, a huge value is capped
        assert!(matches!(
            Width::content(10, 3, 8),
            Width::Flex { min: 8, max: 8, .. }
        ));
        assert!(matches!(
            Width::content(4, 100, 20),
            Width::Flex {
                min: 16,
                max: 20,
                ..
            }
        ));
    }

    #[test]
    fn leftover_beyond_every_cap_stays_unused() {
        let cols = [
            Col {
                id: Id::A,
                header: "A",
                width: Width::content(4, 5, 8),
                priority: 9,
            },
            Col {
                id: Id::B,
                header: "B",
                width: Width::Flex {
                    min: 4,
                    weight: 1,
                    max: 10,
                },
                priority: 10,
            },
        ];
        let planned = plan(&cols, 100);
        assert_eq!((planned[0].width, planned[1].width), (6, 10));
    }

    #[test]
    fn plan_drops_the_least_important_columns_first() {
        // all four need 10+8+6+8+3 = 35
        assert_eq!(ids(&plan(&cols(), 34)), [Id::A, Id::B, Id::D]);
        // without C: 10+8+8+2 = 28
        assert_eq!(ids(&plan(&cols(), 27)), [Id::A, Id::B]);
        assert_eq!(ids(&plan(&cols(), 12)), [Id::B]);
        let one = plan(&cols(), 5);
        assert_eq!(ids(&one), [Id::B]);
        assert_eq!(one[0].width, 5, "a lone column is cut to the space");
    }

    #[test]
    fn plan_never_needs_more_than_it_is_given() {
        for avail in 1..120 {
            let planned = plan(&cols(), avail);
            let used: u16 = planned.iter().map(|p| p.width).sum::<u16>() + planned.len() as u16 - 1;
            assert!(used <= avail.max(8), "avail {avail}: used {used}");
            assert!(!planned.is_empty());
        }
    }

    #[test]
    fn ellipsis_respects_cell_width() {
        assert_eq!(ellipsize("short", 10), "short");
        assert_eq!(ellipsize("a-rather-long-name", 8), "a-rathe…");
        assert_eq!(ellipsize("abc", 1), "…");
        assert_eq!(ellipsize("abc", 0), "");
        // wide characters take two cells
        assert_eq!(ellipsize("日本語テキスト", 7), "日本語…");
        assert!(ellipsize("日本語テキスト", 7).width() <= 7);
    }

    #[test]
    fn wrapping() {
        assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
        assert_eq!(wrap("", 5), [""]);
        assert_eq!(wrap("supercalifragilistic", 8), ["superca…"]);
        assert!(wrap("a b c d e f g", 3).iter().all(|l| l.width() <= 3));
    }
}
