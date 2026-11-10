pub fn validate(pointer: &str) -> Result<(), String> {
    tokens(pointer).map(|_| ())
}

pub fn tokens(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(format!("invalid JSON pointer {pointer:?}"));
    }
    pointer
        .split('/')
        .skip(1)
        .map(|token| {
            let mut characters = token.chars();
            let mut decoded = String::new();
            while let Some(character) = characters.next() {
                match character {
                    '~' => match characters.next() {
                        Some('0') => decoded.push('~'),
                        Some('1') => decoded.push('/'),
                        _ => return Err(format!("invalid JSON pointer escape in {pointer:?}")),
                    },
                    character => decoded.push(character),
                }
            }
            Ok(decoded)
        })
        .collect()
}

pub(crate) fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

pub(crate) fn encode(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| format!("/{}", escape(token)))
        .collect()
}
