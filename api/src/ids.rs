const ALPHABET: [char; 36] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j',
    'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

/// Korte, læsbare id'er med præfiks, fx `ds_k3j9x0a2mq`.
pub fn new(prefix: &str) -> String {
    format!("{prefix}_{}", nanoid::nanoid!(10, &ALPHABET))
}

pub fn prefix_for(kind: &str) -> &'static str {
    match kind {
        "datasource" => "ds",
        "schedule" => "sch",
        "access" => "acc",
        "global" => "gl",
        _ => "pg",
    }
}
