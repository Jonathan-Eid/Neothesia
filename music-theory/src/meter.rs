//! Time signatures, how their beats group, and note value names.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meter {
    pub numerator: u8,
    pub denominator: u8,
}

impl Default for Meter {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

impl Meter {
    pub fn new(numerator: u8, denominator: u8) -> Self {
        Self {
            numerator: numerator.max(1),
            denominator: denominator.max(1),
        }
    }

    pub fn label(&self) -> String {
        format!("{}/{}", self.numerator, self.denominator)
    }

    /// Length of a measure in quarter notes.
    pub fn quarters(&self) -> f32 {
        self.numerator as f32 * 4.0 / self.denominator as f32
    }

    /// An odd meter written in eighths is felt in uneven groups, not as a
    /// stream of equal beats, which is most of what makes it sound the way it does.
    pub fn grouping(&self) -> Vec<u8> {
        if self.denominator != 8 {
            return match (self.numerator, self.denominator) {
                (5, 4) => vec![3, 2],
                (7, 4) => vec![4, 3],
                (numerator, _) => vec![1; numerator as usize],
            };
        }

        match self.numerator {
            5 => vec![3, 2],
            6 => vec![3, 3],
            7 => vec![2, 2, 3],
            8 => vec![3, 3, 2],
            9 => vec![3, 3, 3],
            10 => vec![3, 3, 2, 2],
            11 => vec![3, 3, 3, 2],
            12 => vec![3, 3, 3, 3],
            numerator => vec![1; numerator as usize],
        }
    }

    /// `2+2+3` for 7/8, empty when the beats are even.
    pub fn grouping_label(&self) -> String {
        let grouping = self.grouping();

        if grouping.iter().all(|group| *group == 1) {
            return String::new();
        }

        grouping
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join("+")
    }

    /// `ONE two | ONE two | ONE two three`, the way you would count it out loud.
    pub fn count_label(&self) -> String {
        const SPOKEN: [&str; 5] = ["ONE", "two", "three", "four", "five"];

        self.grouping()
            .iter()
            .map(|group| {
                (0..*group as usize)
                    .map(|beat| SPOKEN[beat.min(4)])
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// Compound meters count in dotted beats, eg. 6/8 is two beats, not six.
    pub fn is_compound(&self) -> bool {
        self.denominator == 8 && self.numerator.is_multiple_of(3) && self.numerator > 3
    }
}

/// Name of a note value, given its length in quarter notes.
pub fn rhythm_name(quarters: f32) -> &'static str {
    const VALUES: [(f32, &str); 16] = [
        (4.0, "whole"),
        (3.0, "dotted half"),
        (2.0, "half"),
        (4.0 / 3.0, "half triplet"),
        (1.5, "dotted quarter"),
        (1.0, "quarter"),
        (0.75, "dotted 8th"),
        (2.0 / 3.0, "quarter triplet"),
        (0.5, "8th"),
        (0.375, "dotted 16th"),
        (1.0 / 3.0, "8th triplet"),
        (0.25, "16th"),
        (1.0 / 6.0, "16th triplet"),
        (0.125, "32nd"),
        (1.0 / 12.0, "32nd triplet"),
        (0.0625, "64th"),
    ];

    if quarters <= 0.0 {
        return "";
    }

    VALUES
        .iter()
        // Compare by ratio, so the tolerance scales with the note value.
        .min_by(|(a, _), (b, _)| {
            let error = |value: f32| (value / quarters).max(quarters / value);
            error(*a).total_cmp(&error(*b))
        })
        .map(|(_, name)| *name)
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_meters_group_unevenly() {
        assert_eq!(Meter::new(7, 8).grouping_label(), "2+2+3");
        assert_eq!(Meter::new(5, 8).grouping_label(), "3+2");
        assert_eq!(Meter::new(6, 8).grouping_label(), "3+3");
        assert_eq!(Meter::new(5, 4).grouping_label(), "3+2");
    }

    #[test]
    fn plain_meters_have_no_grouping() {
        assert_eq!(Meter::new(4, 4).grouping_label(), "");
        assert_eq!(Meter::new(3, 4).grouping_label(), "");
    }

    #[test]
    fn counting_out_loud() {
        assert_eq!(
            Meter::new(7, 8).count_label(),
            "ONE two | ONE two | ONE two three"
        );
    }

    #[test]
    fn measure_length() {
        assert_eq!(Meter::new(4, 4).quarters(), 4.0);
        assert_eq!(Meter::new(7, 8).quarters(), 3.5);
    }

    #[test]
    fn note_values() {
        assert_eq!(rhythm_name(1.0), "quarter");
        assert_eq!(rhythm_name(0.5), "8th");
        assert_eq!(rhythm_name(1.5), "dotted quarter");
        assert_eq!(rhythm_name(1.0 / 3.0), "8th triplet");
        assert_eq!(rhythm_name(0.26), "16th");
        assert_eq!(rhythm_name(0.0), "");
    }
}
