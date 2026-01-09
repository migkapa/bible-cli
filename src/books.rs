pub struct BookDef {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
}

pub fn normalize_book(input: &str) -> Option<&'static str> {
    let key = normalize_key(input);
    for book in BOOKS {
        if normalize_key(book.name) == key {
            return Some(book.name);
        }
        for alias in book.aliases {
            if normalize_key(alias) == key {
                return Some(book.name);
            }
        }
    }
    None
}

fn normalize_key(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub const BOOKS: &[BookDef] = &[
    BookDef { name: "Genesis", aliases: &["gen", "ge", "gn"] },
    BookDef { name: "Exodus", aliases: &["ex", "exo", "exod"] },
    BookDef { name: "Leviticus", aliases: &["lev", "le", "lv"] },
    BookDef { name: "Numbers", aliases: &["num", "nu", "nm", "nb"] },
    BookDef { name: "Deuteronomy", aliases: &["deut", "deu", "dt"] },
    BookDef { name: "Joshua", aliases: &["josh", "jos", "js"] },
    BookDef { name: "Judges", aliases: &["judg", "jdg", "jg"] },
    BookDef { name: "Ruth", aliases: &["ruth", "ru"] },
    BookDef { name: "1 Samuel", aliases: &["1 samuel", "1 sam", "1sa", "i samuel", "i sam", "first samuel", "first sam"] },
    BookDef { name: "2 Samuel", aliases: &["2 samuel", "2 sam", "2sa", "ii samuel", "ii sam", "second samuel", "second sam"] },
    BookDef { name: "1 Kings", aliases: &["1 kings", "1 kgs", "1 kgs", "1ki", "i kings", "first kings", "first kgs"] },
    BookDef { name: "2 Kings", aliases: &["2 kings", "2 kgs", "2ki", "ii kings", "second kings", "second kgs"] },
    BookDef { name: "1 Chronicles", aliases: &["1 chronicles", "1 chron", "1 chr", "1ch", "i chronicles", "first chronicles"] },
    BookDef { name: "2 Chronicles", aliases: &["2 chronicles", "2 chron", "2 chr", "2ch", "ii chronicles", "second chronicles"] },
    BookDef { name: "Ezra", aliases: &["ezra", "ezr"] },
    BookDef { name: "Nehemiah", aliases: &["nehemiah", "neh", "ne"] },
    BookDef { name: "Esther", aliases: &["esther", "est", "es"] },
    BookDef { name: "Job", aliases: &["job", "jb"] },
    BookDef { name: "Psalms", aliases: &["psalms", "psalm", "ps", "psa", "pss"] },
    BookDef { name: "Proverbs", aliases: &["proverbs", "prov", "pr", "prv"] },
    BookDef { name: "Ecclesiastes", aliases: &["ecclesiastes", "eccl", "ecc", "qoheleth"] },
    BookDef { name: "Song of Solomon", aliases: &["song of solomon", "song of songs", "song", "sos", "canticles"] },
    BookDef { name: "Isaiah", aliases: &["isaiah", "isa", "is"] },
    BookDef { name: "Jeremiah", aliases: &["jeremiah", "jer", "je", "jr"] },
    BookDef { name: "Lamentations", aliases: &["lamentations", "lam", "la"] },
    BookDef { name: "Ezekiel", aliases: &["ezekiel", "ezek", "eze", "ezk"] },
    BookDef { name: "Daniel", aliases: &["daniel", "dan", "da", "dn"] },
    BookDef { name: "Hosea", aliases: &["hosea", "hos", "ho"] },
    BookDef { name: "Joel", aliases: &["joel", "jl"] },
    BookDef { name: "Amos", aliases: &["amos", "am"] },
    BookDef { name: "Obadiah", aliases: &["obadiah", "obad", "ob"] },
    BookDef { name: "Jonah", aliases: &["jonah", "jon", "jnh"] },
    BookDef { name: "Micah", aliases: &["micah", "mic", "mc"] },
    BookDef { name: "Nahum", aliases: &["nahum", "nah", "na"] },
    BookDef { name: "Habakkuk", aliases: &["habakkuk", "hab", "hb"] },
    BookDef { name: "Zephaniah", aliases: &["zephaniah", "zeph", "zep", "zp"] },
    BookDef { name: "Haggai", aliases: &["haggai", "hag", "hg"] },
    BookDef { name: "Zechariah", aliases: &["zechariah", "zech", "zec", "zc"] },
    BookDef { name: "Malachi", aliases: &["malachi", "mal", "ml"] },
    BookDef { name: "Matthew", aliases: &["matthew", "matt", "mat", "mt"] },
    BookDef { name: "Mark", aliases: &["mark", "mrk", "mk"] },
    BookDef { name: "Luke", aliases: &["luke", "luk", "lk"] },
    BookDef { name: "John", aliases: &["john", "joh", "jn"] },
    BookDef { name: "Acts", aliases: &["acts", "act", "ac"] },
    BookDef { name: "Romans", aliases: &["romans", "rom", "ro", "rm"] },
    BookDef { name: "1 Corinthians", aliases: &["1 corinthians", "1 cor", "1co", "i corinthians", "first corinthians"] },
    BookDef { name: "2 Corinthians", aliases: &["2 corinthians", "2 cor", "2co", "ii corinthians", "second corinthians"] },
    BookDef { name: "Galatians", aliases: &["galatians", "gal", "ga"] },
    BookDef { name: "Ephesians", aliases: &["ephesians", "eph", "ep"] },
    BookDef { name: "Philippians", aliases: &["philippians", "phil", "php", "phl"] },
    BookDef { name: "Colossians", aliases: &["colossians", "col", "co"] },
    BookDef { name: "1 Thessalonians", aliases: &["1 thessalonians", "1 thess", "1th", "i thessalonians", "first thessalonians"] },
    BookDef { name: "2 Thessalonians", aliases: &["2 thessalonians", "2 thess", "2th", "ii thessalonians", "second thessalonians"] },
    BookDef { name: "1 Timothy", aliases: &["1 timothy", "1 tim", "1ti", "i timothy", "first timothy"] },
    BookDef { name: "2 Timothy", aliases: &["2 timothy", "2 tim", "2ti", "ii timothy", "second timothy"] },
    BookDef { name: "Titus", aliases: &["titus", "tit", "ti"] },
    BookDef { name: "Philemon", aliases: &["philemon", "phm", "phile", "pm"] },
    BookDef { name: "Hebrews", aliases: &["hebrews", "heb", "he"] },
    BookDef { name: "James", aliases: &["james", "jas", "jm"] },
    BookDef { name: "1 Peter", aliases: &["1 peter", "1 pet", "1pe", "i peter", "first peter"] },
    BookDef { name: "2 Peter", aliases: &["2 peter", "2 pet", "2pe", "ii peter", "second peter"] },
    BookDef { name: "1 John", aliases: &["1 john", "1 jn", "1jo", "i john", "first john"] },
    BookDef { name: "2 John", aliases: &["2 john", "2 jn", "2jo", "ii john", "second john"] },
    BookDef { name: "3 John", aliases: &["3 john", "3 jn", "3jo", "iii john", "third john"] },
    BookDef { name: "Jude", aliases: &["jude", "jud"] },
    BookDef { name: "Revelation", aliases: &["revelation", "rev", "re"] },
];
