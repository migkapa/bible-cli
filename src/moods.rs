use crate::verses::VerseRef;

pub struct MoodDef {
    pub name: &'static str,
    pub description: &'static str,
    pub refs: &'static [VerseRef],
}

pub fn all_moods() -> &'static [MoodDef] {
    MOODS
}

pub fn find_mood(name: &str) -> Option<&'static MoodDef> {
    let key = normalize_key(name);
    MOODS.iter().find(|m| normalize_key(m.name) == key)
}

fn normalize_key(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const MOODS: &[MoodDef] = &[
    MoodDef {
        name: "peace",
        description: "Rest and calm in the storm",
        refs: &[
            VerseRef {
                book: "John",
                chapter: 14,
                verse: 27,
            },
            VerseRef {
                book: "Philippians",
                chapter: 4,
                verse: 6,
            },
            VerseRef {
                book: "Psalms",
                chapter: 23,
                verse: 1,
            },
            VerseRef {
                book: "Isaiah",
                chapter: 26,
                verse: 3,
            },
            VerseRef {
                book: "Matthew",
                chapter: 11,
                verse: 28,
            },
        ],
    },
    MoodDef {
        name: "courage",
        description: "Strength for hard steps",
        refs: &[
            VerseRef {
                book: "Joshua",
                chapter: 1,
                verse: 9,
            },
            VerseRef {
                book: "Isaiah",
                chapter: 41,
                verse: 10,
            },
            VerseRef {
                book: "Psalms",
                chapter: 27,
                verse: 1,
            },
            VerseRef {
                book: "2 Timothy",
                chapter: 1,
                verse: 7,
            },
            VerseRef {
                book: "Deuteronomy",
                chapter: 31,
                verse: 6,
            },
        ],
    },
    MoodDef {
        name: "wisdom",
        description: "Guidance and clarity",
        refs: &[
            VerseRef {
                book: "Proverbs",
                chapter: 3,
                verse: 5,
            },
            VerseRef {
                book: "James",
                chapter: 1,
                verse: 5,
            },
            VerseRef {
                book: "Proverbs",
                chapter: 9,
                verse: 10,
            },
            VerseRef {
                book: "Ecclesiastes",
                chapter: 7,
                verse: 12,
            },
            VerseRef {
                book: "Psalms",
                chapter: 111,
                verse: 10,
            },
        ],
    },
    MoodDef {
        name: "hope",
        description: "Light ahead",
        refs: &[
            VerseRef {
                book: "Romans",
                chapter: 15,
                verse: 13,
            },
            VerseRef {
                book: "Jeremiah",
                chapter: 29,
                verse: 11,
            },
            VerseRef {
                book: "Psalms",
                chapter: 42,
                verse: 11,
            },
            VerseRef {
                book: "Hebrews",
                chapter: 11,
                verse: 1,
            },
            VerseRef {
                book: "Lamentations",
                chapter: 3,
                verse: 22,
            },
        ],
    },
    MoodDef {
        name: "gratitude",
        description: "Thanks and remembrance",
        refs: &[
            VerseRef {
                book: "1 Thessalonians",
                chapter: 5,
                verse: 18,
            },
            VerseRef {
                book: "Psalms",
                chapter: 100,
                verse: 4,
            },
            VerseRef {
                book: "Colossians",
                chapter: 3,
                verse: 15,
            },
            VerseRef {
                book: "Psalms",
                chapter: 107,
                verse: 1,
            },
            VerseRef {
                book: "Philippians",
                chapter: 4,
                verse: 4,
            },
        ],
    },
];
