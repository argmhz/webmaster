//! Formateret tekst gemmes som ProseMirror-JSON med et fast skema, som editoren
//! (`editor/src/RichText.tsx`) og sitet deler:
//!
//! ```json
//! { "type": "doc", "content": [
//!   { "type": "paragraph", "content": [
//!     { "type": "text", "text": "Hej " },
//!     { "type": "text", "text": "verden", "marks": [{ "type": "bold" }] }
//!   ] }
//! ] }
//! ```
//!
//! Blokke: `paragraph`, `heading` (`attrs.level` 2–4), `bulletList`, `orderedList`
//! (`attrs.start`), `listItem`, `blockquote`, `horizontalRule`.
//! Inline: `text`, `hardBreak`. Marks: `bold`, `italic`, `underline`, `strike`,
//! `link` (`attrs.href`, `attrs.target` = `"_blank"` eller null).
//!
//! Skemaet er bevidst lukket, så sitet kan rendere alt, der kan gemmes.

use serde_json::Value;

pub const BLOCKS: [&str; 7] = ["paragraph", "heading", "bulletList", "orderedList", "listItem", "blockquote", "horizontalRule"];
pub const INLINE: [&str; 2] = ["text", "hardBreak"];
pub const MARKS: [&str; 5] = ["bold", "italic", "underline", "strike", "link"];

/// Tjekker et dokument. Giver den første fejl i klar tekst.
pub fn check(v: &Value) -> Option<String> {
    let Some(obj) = v.as_object() else { return Some("skal være formateret tekst (et ProseMirror-dokument)".into()) };
    if obj.get("type").and_then(Value::as_str) != Some("doc") {
        return Some("skal være et ProseMirror-dokument med \"type\": \"doc\"".into());
    }
    block_content(v, "dokumentet", false).err()
}

/// Antal tegn i teksten. Bruges til `minLength`, `maxLength` og `required`.
pub fn text_len(v: &Value) -> usize {
    match v {
        Value::Object(m) => {
            let own = m.get("text").and_then(Value::as_str).map(|s| s.chars().count()).unwrap_or(0);
            own + m.get("content").map(text_len).unwrap_or(0)
        }
        Value::Array(a) => a.iter().map(text_len).sum(),
        _ => 0,
    }
}

/// Tom: ingen tekst og ingen streger. Et dokument med kun tomme afsnit er tomt.
pub fn is_empty(v: &Value) -> bool {
    fn has_rule(v: &Value) -> bool {
        match v {
            Value::Object(m) => {
                m.get("type").and_then(Value::as_str) == Some("horizontalRule") || m.get("content").is_some_and(has_rule)
            }
            Value::Array(a) => a.iter().any(has_rule),
            _ => false,
        }
    }
    text_len(v) == 0 && !has_rule(v)
}

type Res = Result<(), String>;

fn content(node: &Value) -> Result<&[Value], String> {
    match node.get("content") {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err("«content» skal være en liste".into()),
    }
}

fn ty(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or("")
}

/// Blokke inde i `doc`, `listItem` og `blockquote`.
fn block_content(parent: &Value, what: &str, require: bool) -> Res {
    let c = content(parent)?;
    if require && c.is_empty() {
        return Err(format!("{what} må ikke være tom"));
    }
    for n in c {
        block(n)?;
    }
    Ok(())
}

fn block(n: &Value) -> Res {
    if !n.is_object() {
        return Err("en blok skal være et objekt".into());
    }
    match ty(n) {
        "paragraph" => inline_content(n),
        "heading" => {
            let level = n.pointer("/attrs/level").and_then(Value::as_u64);
            if !matches!(level, Some(2..=4)) {
                return Err("en overskrift skal have niveau 2, 3 eller 4".into());
            }
            inline_content(n)
        }
        "bulletList" | "orderedList" => {
            if let Some(s) = n.pointer("/attrs/start") {
                if !s.is_null() && s.as_u64().is_none() {
                    return Err("«start» på en nummereret liste skal være et tal".into());
                }
            }
            let items = content(n)?;
            if items.is_empty() {
                return Err("en liste skal have mindst ét punkt".into());
            }
            for i in items {
                if ty(i) != "listItem" {
                    return Err(format!("en liste må kun indeholde listItem, ikke «{}»", ty(i)));
                }
                block_content(i, "et listepunkt", true)?;
            }
            Ok(())
        }
        "blockquote" => block_content(n, "et citat", true),
        "horizontalRule" => match content(n)?.is_empty() {
            true => Ok(()),
            false => Err("en streg kan ikke have indhold".into()),
        },
        "listItem" => Err("listItem må kun stå inde i en liste".into()),
        "text" | "hardBreak" => Err(format!("«{}» skal stå inde i et afsnit eller en overskrift", ty(n))),
        t => Err(format!("blokken «{t}» findes ikke. Tilladt: {}", BLOCKS.join(", "))),
    }
}

fn inline_content(n: &Value) -> Res {
    for i in content(n)? {
        match ty(i) {
            "text" => {
                match i.get("text").and_then(Value::as_str) {
                    Some(t) if !t.is_empty() => {}
                    _ => return Err("en tekst-node skal have en ikke-tom «text»".into()),
                }
                marks(i)?;
            }
            "hardBreak" => {}
            t => return Err(format!("«{t}» kan ikke stå inde i et afsnit. Tilladt: {}", INLINE.join(", "))),
        }
    }
    Ok(())
}

fn marks(n: &Value) -> Res {
    let ms = match n.get("marks") {
        None | Some(Value::Null) => return Ok(()),
        Some(Value::Array(a)) => a,
        Some(_) => return Err("«marks» skal være en liste".into()),
    };
    for m in ms {
        match ty(m) {
            "bold" | "italic" | "underline" | "strike" => {}
            "link" => {
                let href = m.pointer("/attrs/href").and_then(Value::as_str).unwrap_or("");
                if !crate::validate::is_link(href) {
                    return Err(format!("linket «{href}» er ikke et link. Brug /sti, #anker eller https://…"));
                }
                match m.pointer("/attrs/target") {
                    None | Some(Value::Null) => {}
                    Some(Value::String(s)) if s == "_blank" => {}
                    Some(_) => return Err("«target» på et link skal være \"_blank\" eller null".into()),
                }
            }
            t => return Err(format!("formateringen «{t}» findes ikke. Tilladt: {}", MARKS.join(", "))),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(text: &str) -> Value {
        json!({ "type": "paragraph", "content": [{ "type": "text", "text": text }] })
    }

    #[test]
    fn valid_documents() {
        let doc = json!({ "type": "doc", "content": [
            { "type": "heading", "attrs": { "level": 2 }, "content": [{ "type": "text", "text": "Titel" }] },
            p("Hej"),
            { "type": "paragraph" },
            { "type": "bulletList", "content": [{ "type": "listItem", "content": [p("a")] }] },
            { "type": "orderedList", "attrs": { "start": 3 }, "content": [{ "type": "listItem", "content": [p("b")] }] },
            { "type": "blockquote", "content": [p("citat")] },
            { "type": "horizontalRule" },
            { "type": "paragraph", "content": [
                { "type": "text", "text": "link", "marks": [{ "type": "link", "attrs": { "href": "https://borsen.dk", "target": "_blank" } }, { "type": "bold" }] },
                { "type": "hardBreak" }
            ] }
        ] });
        assert_eq!(check(&doc), None);
        assert_eq!(text_len(&doc), "TitelHejabcitatlink".chars().count());
        assert!(!is_empty(&doc));
        assert!(is_empty(&json!({ "type": "doc", "content": [{ "type": "paragraph" }] })));
        assert!(!is_empty(&json!({ "type": "doc", "content": [{ "type": "horizontalRule" }] })));
    }

    #[test]
    fn invalid_documents() {
        let bad = |content: Value| check(&json!({ "type": "doc", "content": content })).unwrap_or_default();
        assert!(check(&json!("tekst")).is_some());
        assert!(check(&json!({ "type": "paragraph" })).unwrap().contains("doc"));
        assert!(bad(json!([{ "type": "heading", "attrs": { "level": 1 } }])).contains("niveau"));
        assert!(bad(json!([{ "type": "image" }])).contains("«image» findes ikke"));
        assert!(bad(json!([{ "type": "text", "text": "x" }])).contains("inde i et afsnit"));
        assert!(bad(json!([{ "type": "bulletList", "content": [p("x")] }])).contains("kun indeholde listItem"));
        assert!(bad(json!([{ "type": "paragraph", "content": [{ "type": "text", "text": "x", "marks": [{ "type": "link", "attrs": { "href": "www.dr.dk" } }] }] }])).contains("www.dr.dk"));
        assert!(bad(json!([{ "type": "paragraph", "content": [{ "type": "text", "text": "x", "marks": [{ "type": "code" }] }] }])).contains("«code»"));
        assert!(bad(json!([{ "type": "paragraph", "content": [{ "type": "text", "text": "" }] }])).contains("ikke-tom"));
    }
}
