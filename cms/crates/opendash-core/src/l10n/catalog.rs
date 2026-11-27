use std::collections::{BTreeMap, BTreeSet};

use super::{L10nError, PluralRule};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub msgid_plural: Option<String>,
    pub translations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalog {
    plural_rule: PluralRule,
    entries: BTreeMap<(Option<String>, String), CatalogEntry>,
}

pub trait CatalogLookup {
    fn lookup(&self, context: Option<&str>, msgid: &str) -> Option<&str>;
    fn lookup_plural(
        &self,
        context: Option<&str>,
        singular: &str,
        plural: &str,
        count: i64,
    ) -> Option<&str>;
}

impl CatalogLookup for Catalog {
    fn lookup(&self, context: Option<&str>, msgid: &str) -> Option<&str> {
        self.get_with_context(context, msgid)
    }

    fn lookup_plural(
        &self,
        context: Option<&str>,
        singular: &str,
        plural: &str,
        count: i64,
    ) -> Option<&str> {
        let _ = plural;
        self.get_plural_with_context(context, singular, count)
    }
}

impl Catalog {
    pub fn from_parts(
        plural_rule: PluralRule,
        entries: BTreeMap<(Option<String>, String), CatalogEntry>,
    ) -> Self {
        Self {
            plural_rule,
            entries,
        }
    }

    pub fn plural_rule(&self) -> &PluralRule {
        &self.plural_rule
    }

    pub fn entries(&self) -> &BTreeMap<(Option<String>, String), CatalogEntry> {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get<'a>(&'a self, msgid: &'a str) -> &'a str {
        self.get_with_context(None, msgid).unwrap_or(msgid)
    }

    pub fn get_context<'a>(&'a self, context: &str, msgid: &'a str) -> &'a str {
        self.get_with_context(Some(context), msgid).unwrap_or(msgid)
    }

    pub fn get_with_context(&self, context: Option<&str>, msgid: &str) -> Option<&str> {
        let key = (context.map(str::to_owned), msgid.to_owned());
        self.entries
            .get(&key)
            .and_then(|e| e.translations.first())
            .filter(|s| !s.is_empty())
            .map(String::as_str)
    }

    pub fn get_plural<'a>(&'a self, singular: &'a str, plural: &'a str, count: i64) -> &'a str {
        self.get_plural_with_context(None, singular, count)
            .unwrap_or(if count == 1 { singular } else { plural })
    }

    pub fn get_plural_with_context(
        &self,
        context: Option<&str>,
        singular: &str,
        count: i64,
    ) -> Option<&str> {
        let key = (context.map(str::to_owned), singular.to_owned());
        let entry = self.entries.get(&key)?;
        if entry.translations.is_empty() {
            return None;
        }
        let idx = self.plural_rule.evaluate(count, entry.translations.len());
        entry
            .translations
            .get(idx)
            .filter(|s| !s.is_empty())
            .map(String::as_str)
    }

    /// Computes translation coverage percentage (`0..=100`) against a `.pot` template.
    pub fn coverage_against_pot(&self, pot: &PotDocument) -> u8 {
        if pot.entries.is_empty() {
            return 100;
        }
        let translated = pot
            .entries
            .keys()
            .filter(|(ctx, msgid)| {
                self.entries
                    .get(&(ctx.clone(), msgid.clone()))
                    .is_some_and(|e| e.translations.iter().any(|s| !s.is_empty()))
            })
            .count();
        ((translated * 100) / pot.entries.len()).min(100) as u8
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PotEntry {
    pub msgctxt: Option<String>,
    pub msgid: String,
    pub msgid_plural: Option<String>,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PotDocument {
    pub entries: BTreeMap<(Option<String>, String), PotEntry>,
}

impl PotDocument {
    pub fn parse(pot_text: &str) -> Result<Self, L10nError> {
        let mut entries = BTreeMap::new();
        let clean = pot_text.strip_prefix('\u{FEFF}').unwrap_or(pot_text);
        let mut ctx: Option<String> = None;
        let mut id: Option<String> = None;
        let mut plural: Option<String> = None;
        let mut refs: Vec<String> = Vec::new();
        let mut mode: u8 = 0;

        let flush = |entries: &mut BTreeMap<(Option<String>, String), PotEntry>,
                     ctx: &mut Option<String>,
                     id: &mut Option<String>,
                     plural: &mut Option<String>,
                     refs: &mut Vec<String>| {
            if let Some(msgid) = id.take() {
                let msgctxt = ctx.take();
                let msgid_plural = plural.take();
                let references = std::mem::take(refs);
                if !msgid.is_empty() {
                    entries.insert(
                        (msgctxt.clone(), msgid.clone()),
                        PotEntry {
                            msgctxt,
                            msgid,
                            msgid_plural,
                            references,
                        },
                    );
                }
            } else {
                *ctx = None;
                *plural = None;
                refs.clear();
            }
        };

        for raw in clean.lines() {
            let line = raw.trim_end_matches('\r').trim();
            if line.is_empty() {
                flush(&mut entries, &mut ctx, &mut id, &mut plural, &mut refs);
                mode = 0;
                continue;
            }
            if let Some(r) = line.strip_prefix("#:") {
                refs.push(r.trim().to_owned());
                continue;
            }
            if line.starts_with('#') || line.starts_with("msgstr") {
                mode = 0;
                continue;
            }
            if let Some(rest) = line.strip_prefix("msgctxt") {
                ctx = Some(unquote_pot(rest)?);
                mode = 1;
            } else if let Some(rest) = line.strip_prefix("msgid_plural") {
                plural = Some(unquote_pot(rest)?);
                mode = 3;
            } else if let Some(rest) = line.strip_prefix("msgid") {
                id = Some(unquote_pot(rest)?);
                mode = 2;
            } else if line.starts_with('"') {
                let cont = unquote_pot(line)?;
                match mode {
                    1 => {
                        if let Some(v) = ctx.as_mut() {
                            v.push_str(&cont);
                        }
                    }
                    2 => {
                        if let Some(v) = id.as_mut() {
                            v.push_str(&cont);
                        }
                    }
                    3 => {
                        if let Some(v) = plural.as_mut() {
                            v.push_str(&cont);
                        }
                    }
                    _ => {}
                }
            }
        }
        flush(&mut entries, &mut ctx, &mut id, &mut plural, &mut refs);
        Ok(Self { entries })
    }
}

fn unquote_pot(raw: &str) -> Result<String, L10nError> {
    let t = raw.trim();
    if !t.starts_with('"') || !t.ends_with('"') || t.len() < 2 {
        return Err(L10nError::new("Chuỗi .pot không hợp lệ."));
    }
    Ok(t[1..t.len() - 1]
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\r", "\r")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\"))
}

/// Validates `{name}`, `{{name}}`, `%s`, `%d` placeholder parity between source and translation.
pub fn validate_placeholders(source: &str, translated: &str) -> Result<(), L10nError> {
    let expected = extract_placeholders(source);
    let actual = extract_placeholders(translated);
    if expected != actual {
        return Err(L10nError::new(format!(
            "Placeholder không khớp cho '{source}': cần {expected:?}, nhận {actual:?}."
        )));
    }
    Ok(())
}

fn extract_placeholders(s: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = s[i + 2..].find("}}") {
                let name = s[i + 2..i + 2 + end].trim();
                if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    set.insert(format!("{{{{{name}}}}}"));
                }
                i += 2 + end + 2;
                continue;
            }
        } else if bytes[i] == b'{' {
            if let Some(end) = s[i + 1..].find('}') {
                let name = s[i + 1..i + 1 + end].trim();
                if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    set.insert(format!("{{{name}}}"));
                }
                i += 1 + end + 1;
                continue;
            }
        } else if bytes[i] == b'%' && i + 1 < bytes.len() && matches!(bytes[i + 1], b's' | b'd') {
            set.insert(format!("%{}", bytes[i + 1] as char));
            i += 2;
            continue;
        }
        i += 1;
    }
    set
}
