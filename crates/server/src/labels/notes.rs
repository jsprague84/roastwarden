//! The tasting notes a label prints: the bean's own, else the roast's cupping notes, else the
//! supplier's prose cut at a sentence, and the flavour words a suggestion is made from.

/// The label's tasting notes: the bean's label notes, else the roast's own cupping notes,
/// else the supplier's notes cut at a sentence (`prose_notes`)
pub fn notes_for(
    label_notes: Option<&str>,
    cupping: Option<&str>,
    supplier: Option<&str>,
) -> Option<String> {
    let there = |s: Option<&str>| {
        s.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    there(label_notes)
        .or_else(|| there(cupping))
        .or_else(|| there(supplier).map(|p| prose_notes(&p)))
}

/// A supplier's notes short enough for a label: whole sentences up to about 140 characters,
/// else the first sentence cut after a clause (", ", "; ") with "…", never mid-word
pub fn prose_notes(prose: &str) -> String {
    const MAX: usize = 140;
    let prose = prose.split_whitespace().collect::<Vec<_>>().join(" ");
    if prose.chars().count() <= MAX {
        return prose;
    }
    // Sentence ends: ". ", "! ", "? " (not "St. ": a capital must follow)
    let chars: Vec<char> = prose.chars().collect();
    let mut kept = 0;
    for i in 0..chars.len().saturating_sub(2) {
        if matches!(chars[i], '.' | '!' | '?') && chars[i + 1] == ' ' && chars[i + 2].is_uppercase()
        {
            if i < MAX {
                kept = i + 1;
            } else {
                break;
            }
        }
    }
    if kept >= 40 {
        return chars[..kept].iter().collect();
    }
    let head: String = chars[..MAX].iter().collect();
    let cut = head
        .rfind([',', ';'])
        .filter(|&i| i >= 40)
        .or_else(|| head.rfind(' '))
        .unwrap_or(head.len());
    format!("{}…", head[..cut].trim_end_matches([',', ';', ' ']))
}

/// Tasting words found in a supplier's notes, in their order, at most four, as a bean's
/// label notes would read: "Milk chocolate · dried fig · cocoa". None with fewer than two.
pub fn suggest_notes(prose: &str) -> Option<String> {
    let low = prose.to_lowercase();
    let mut found: Vec<(usize, usize, &str)> = Vec::new();
    // Longest first, so "milk chocolate" is one word and not "chocolate" too
    let mut words: Vec<&str> = FLAVOURS.to_vec();
    words.sort_by_key(|w| std::cmp::Reverse(w.len()));
    for w in words {
        let mut from = 0;
        while let Some(at) = low[from..].find(w).map(|i| i + from) {
            let end = at + w.len();
            let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
            let whole = edge(low[..at].chars().next_back()) && edge(low[end..].chars().next());
            let free = found.iter().all(|(a, b, _)| end <= *a || at >= *b);
            if whole && free {
                found.push((at, end, w));
                break;
            }
            from = end;
        }
    }
    found.sort_by_key(|f| f.0);
    found.dedup_by_key(|f| f.2);
    if found.len() < 2 {
        return None;
    }
    let mut out = found
        .iter()
        .take(4)
        .map(|f| f.2.to_string())
        .collect::<Vec<_>>()
        .join(" · ");
    if let Some(c) = out.get(..1) {
        out = c.to_uppercase() + &out[1..];
    }
    Some(out)
}

/// Tasting words a label may carry (the SCA flavour wheel's, and what suppliers write)
const FLAVOURS: &[&str] = &[
    "milk chocolate",
    "dark chocolate",
    "chocolate",
    "cocoa",
    "cacao nib",
    "brownie",
    "caramel",
    "toffee",
    "butterscotch",
    "brown sugar",
    "molasses",
    "panela",
    "honey",
    "maple",
    "vanilla",
    "nougat",
    "marzipan",
    "almond",
    "hazelnut",
    "peanut",
    "walnut",
    "pecan",
    "praline",
    "red apple",
    "green apple",
    "apple",
    "pear",
    "peach",
    "apricot",
    "nectarine",
    "plum",
    "cherry",
    "black cherry",
    "stone fruit",
    "red grape",
    "grape",
    "raisin",
    "date",
    "dried fig",
    "fig",
    "prune",
    "strawberry",
    "raspberry",
    "blueberry",
    "blackberry",
    "blackcurrant",
    "black currant",
    "red currant",
    "cranberry",
    "berry",
    "lemon",
    "lime",
    "orange",
    "mandarin",
    "tangerine",
    "grapefruit",
    "bergamot",
    "citrus",
    "yuzu",
    "pineapple",
    "mango",
    "papaya",
    "passion fruit",
    "guava",
    "lychee",
    "melon",
    "watermelon",
    "kiwi",
    "banana",
    "coconut",
    "tropical fruit",
    "pomegranate",
    "tamarind",
    "rhubarb",
    "tomato",
    "jasmine",
    "rose",
    "lavender",
    "hibiscus",
    "orange blossom",
    "chamomile",
    "floral",
    "black tea",
    "green tea",
    "earl grey",
    "tea",
    "cinnamon",
    "clove",
    "nutmeg",
    "cardamom",
    "ginger",
    "black pepper",
    "spice",
    "licorice",
    "tobacco",
    "cedar",
    "malt",
    "graham cracker",
    "biscuit",
    "wine",
    "winey",
    "rum",
    "whiskey",
    "cola",
    "sugarcane",
    "cane sugar",
    "candied",
    "jammy",
    "syrup",
];
