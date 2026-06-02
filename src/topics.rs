use crate::verses::VerseRef;

/// A curated, doctrine/study-oriented verse collection — deeper and more
/// thematic than the emotional `moods` sets. Useful for sermon and study prep.
pub struct TopicDef {
    pub name: &'static str,
    pub description: &'static str,
    pub refs: &'static [VerseRef],
}

pub fn all_topics() -> &'static [TopicDef] {
    TOPICS
}

pub fn find_topic(name: &str) -> Option<&'static TopicDef> {
    let key = normalize_key(name);
    TOPICS.iter().find(|t| normalize_key(t.name) == key)
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

const fn r(book: &'static str, chapter: u16, verse: u16) -> VerseRef {
    VerseRef {
        book,
        chapter,
        verse,
    }
}

const TOPICS: &[TopicDef] = &[
    TopicDef {
        name: "faith",
        description: "Trust and belief in God",
        refs: &[
            r("Hebrews", 11, 1),
            r("Romans", 10, 17),
            r("Ephesians", 2, 8),
            r("Mark", 11, 22),
            r("2 Corinthians", 5, 7),
            r("James", 2, 17),
        ],
    },
    TopicDef {
        name: "grace",
        description: "Unmerited favor and mercy",
        refs: &[
            r("Ephesians", 2, 8),
            r("2 Corinthians", 12, 9),
            r("Romans", 5, 20),
            r("Titus", 2, 11),
            r("Hebrews", 4, 16),
        ],
    },
    TopicDef {
        name: "love",
        description: "The love of God and love for one another",
        refs: &[
            r("1 Corinthians", 13, 4),
            r("John", 3, 16),
            r("1 John", 4, 7),
            r("Romans", 5, 8),
            r("John", 13, 34),
        ],
    },
    TopicDef {
        name: "forgiveness",
        description: "Pardon, mercy, and reconciliation",
        refs: &[
            r("1 John", 1, 9),
            r("Ephesians", 4, 32),
            r("Colossians", 3, 13),
            r("Matthew", 6, 14),
            r("Psalms", 103, 12),
        ],
    },
    TopicDef {
        name: "salvation",
        description: "Redemption and eternal life in Christ",
        refs: &[
            r("Romans", 10, 9),
            r("John", 3, 16),
            r("Acts", 4, 12),
            r("Ephesians", 2, 8),
            r("Romans", 6, 23),
        ],
    },
    TopicDef {
        name: "resurrection",
        description: "Christ's rising and the hope of new life",
        refs: &[
            r("1 Corinthians", 15, 20),
            r("John", 11, 25),
            r("Romans", 6, 4),
            r("1 Peter", 1, 3),
            r("Matthew", 28, 6),
        ],
    },
    TopicDef {
        name: "prayer",
        description: "Communion with God in petition and thanks",
        refs: &[
            r("Philippians", 4, 6),
            r("Matthew", 6, 9),
            r("1 Thessalonians", 5, 17),
            r("James", 5, 16),
            r("Matthew", 7, 7),
        ],
    },
    TopicDef {
        name: "wisdom",
        description: "Understanding and the fear of the Lord",
        refs: &[
            r("Proverbs", 1, 7),
            r("James", 1, 5),
            r("Proverbs", 3, 5),
            r("Ecclesiastes", 7, 12),
            r("Colossians", 2, 3),
        ],
    },
    TopicDef {
        name: "hope",
        description: "Confident expectation in God's promises",
        refs: &[
            r("Romans", 15, 13),
            r("Jeremiah", 29, 11),
            r("Hebrews", 6, 19),
            r("Romans", 5, 5),
            r("Psalms", 39, 7),
        ],
    },
    TopicDef {
        name: "strength",
        description: "God's power for the weary",
        refs: &[
            r("Isaiah", 40, 31),
            r("Philippians", 4, 13),
            r("Psalms", 46, 1),
            r("2 Corinthians", 12, 9),
            r("Nehemiah", 8, 10),
        ],
    },
];
