//! Attributed, pinned offline terms. No external contexts or runtime downloads.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

pub const RELEASE: &str = "30.1";
const HASH: &str = "82199ae61f4d7b4bc0c155d11fc20fa252de8bb690bd50d9eb59a4ef06e8ce6e";
const SOURCE: &str = "0a4b8c4c910fc831ec8695196510973390906a5ee34dc6b862dfec4d21a12f74";
const DATA: &[u8] = include_bytes!("../../assets/schemaorg-v30.1.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Type {
    pub parents: Vec<String>,
    pub datatype: bool,
    pub pending: bool,
    pub superseded_by: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Property {
    pub parents: Vec<String>,
    pub domains: Vec<String>,
    pub ranges: Vec<String>,
    pub pending: bool,
    pub superseded_by: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Enumeration {
    pub types: Vec<String>,
    pub pending: bool,
    pub superseded_by: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    release: String,
    source_sha256: String,
    license: String,
    types: BTreeMap<String, Type>,
    properties: BTreeMap<String, Property>,
    enumerations: BTreeMap<String, Enumeration>,
}
pub struct Vocabulary {
    pub types: BTreeMap<String, Type>,
    pub properties: BTreeMap<String, Property>,
    pub enumerations: BTreeMap<String, Enumeration>,
    ancestors: BTreeMap<String, BTreeSet<String>>,
    indexes: BTreeMap<&'static str, Vec<(String, String)>>,
}
#[derive(Serialize)]
pub struct Item {
    pub name: String,
    pub pending: bool,
    pub domains: Vec<String>,
    pub ranges: Vec<String>,
    pub datatype: bool,
    pub enumeration: bool,
}
#[derive(Serialize)]
pub struct Page {
    pub release: &'static str,
    pub license: &'static str,
    pub items: Vec<Item>,
    pub after: Option<String>,
}
fn invalid() -> Error {
    Error::invalid("The pinned offline vocabulary is invalid.")
}
fn ancestry(
    name: &str,
    types: &BTreeMap<String, Type>,
    visiting: &mut BTreeSet<String>,
    memo: &mut BTreeMap<String, BTreeSet<String>>,
) -> Result<BTreeSet<String>> {
    if let Some(value) = memo.get(name) {
        return Ok(value.clone());
    }
    if visiting.len() >= 64 || !visiting.insert(name.into()) {
        return Err(invalid());
    }
    let mut values = BTreeSet::from([name.to_owned()]);
    let term = types.get(name).ok_or_else(invalid)?;
    for parent in &term.parents {
        values.extend(ancestry(parent, types, visiting, memo)?);
    }
    visiting.remove(name);
    memo.insert(name.into(), values.clone());
    Ok(values)
}
impl Vocabulary {
    fn parse() -> Result<Self> {
        if DATA.len() > 512 * 1024 || crate::auth::digest(DATA) != HASH {
            return Err(invalid());
        }
        let raw: Raw = serde_json::from_slice(DATA).map_err(|_| invalid())?;
        if raw.release != RELEASE
            || raw.source_sha256 != SOURCE
            || raw.license != "CC-BY-SA-3.0; Schema.org vocabulary data"
        {
            return Err(invalid());
        }
        let mut ancestors = BTreeMap::new();
        for name in raw.types.keys() {
            ancestry(name, &raw.types, &mut BTreeSet::new(), &mut ancestors)?;
        }
        for term in raw.properties.values() {
            if term
                .domains
                .iter()
                .chain(&term.ranges)
                .any(|n| !raw.types.contains_key(n))
                || term.parents.iter().any(|n| !raw.properties.contains_key(n))
            {
                return Err(invalid());
            }
        }
        for term in raw.enumerations.values() {
            if term.types.iter().any(|n| !raw.types.contains_key(n)) {
                return Err(invalid());
            }
        }
        let mut indexes = BTreeMap::new();
        for (kind, names) in [
            ("type", raw.types.keys().collect::<Vec<_>>()),
            ("property", raw.properties.keys().collect()),
            ("enumeration", raw.enumerations.keys().collect()),
        ] {
            let mut index: Vec<_> = names
                .into_iter()
                .map(|n| (n.to_ascii_lowercase(), n.clone()))
                .collect();
            index.sort();
            indexes.insert(kind, index);
        }
        Ok(Self {
            types: raw.types,
            properties: raw.properties,
            enumerations: raw.enumerations,
            ancestors,
            indexes,
        })
    }
    pub fn is_a(&self, name: &str, ancestor: &str) -> bool {
        self.ancestors
            .get(name)
            .is_some_and(|s| s.contains(ancestor))
    }
    pub fn datatype(&self, name: &str) -> bool {
        self.ancestors
            .get(name)
            .is_some_and(|a| a.iter().any(|n| self.types[n].datatype))
    }
    pub fn entity_type(&self, name: &str, pending: bool) -> Result<()> {
        let term = self.types.get(name).ok_or_else(|| {
            Error::invalid("Choose a known entity type from the offline vocabulary.")
        })?;
        if self.datatype(name)
            || self.is_a(name, "Enumeration")
            || !term.superseded_by.is_empty()
            || (term.pending && !pending)
        {
            return Err(Error::invalid(
                "Choose an active entity type; pending terms require explicit admission.",
            ));
        }
        Ok(())
    }
    pub fn property(&self, name: &str, pending: bool) -> Result<&Property> {
        let term = self.properties.get(name).ok_or_else(|| {
            Error::invalid("Choose a known property from the offline vocabulary.")
        })?;
        if !term.superseded_by.is_empty() || (term.pending && !pending) {
            return Err(Error::invalid(
                "Retired properties cannot be selected; pending terms require explicit admission.",
            ));
        }
        Ok(term)
    }
    pub fn enumeration(&self, name: &str, ranges: &[String], pending: bool) -> Result<()> {
        let term = self
            .enumerations
            .get(name)
            .ok_or_else(|| Error::invalid("Choose a known enumeration value."))?;
        if !term.superseded_by.is_empty()
            || (term.pending && !pending)
            || !term
                .types
                .iter()
                .any(|t| ranges.iter().any(|r| self.is_a(t, r)))
        {
            return Err(Error::invalid(
                "Enumeration value does not match the admitted property range.",
            ));
        }
        Ok(())
    }
    pub fn associated(&self, types: &[String], property: &Property) -> bool {
        types
            .iter()
            .any(|t| self.is_a(t, "Role") || property.domains.iter().any(|d| self.is_a(t, d)))
    }
    pub fn search(&self, family: &str, prefix: &str, after: &str, pending: bool) -> Result<Page> {
        if after.len() > 100
            || prefix.len() > 64
            || !prefix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b' ')
        {
            return Err(Error::invalid(
                "Search a term-name prefix of at most 64 characters.",
            ));
        }
        let index = self
            .indexes
            .get(family)
            .ok_or_else(|| Error::invalid("Choose type, property or enumeration."))?;
        let prefix = prefix.replace(' ', "").to_ascii_lowercase();
        let mut start = index.partition_point(|(lower, _)| lower < &prefix);
        if !after.is_empty() {
            let cursor = (after.to_ascii_lowercase(), after.to_owned());
            if !cursor.0.starts_with(&prefix) || index.binary_search(&cursor).is_err() {
                return Err(Error::invalid(
                    "Vocabulary cursor does not match this search.",
                ));
            }
            start = index.partition_point(|entry| entry <= &cursor);
        }
        let mut items = Vec::new();
        for (_, name) in index
            .iter()
            .skip(start)
            .take_while(|(lower, _)| lower.starts_with(&prefix))
        {
            let (is_pending, retired, domains, ranges, datatype, enumeration) = match family {
                "type" => {
                    let t = &self.types[name];
                    (
                        t.pending,
                        !t.superseded_by.is_empty(),
                        Vec::new(),
                        Vec::new(),
                        self.datatype(name),
                        self.is_a(name, "Enumeration"),
                    )
                }
                "property" => {
                    let t = &self.properties[name];
                    (
                        t.pending,
                        !t.superseded_by.is_empty(),
                        t.domains.clone(),
                        t.ranges.clone(),
                        false,
                        false,
                    )
                }
                _ => {
                    let t = &self.enumerations[name];
                    (
                        t.pending,
                        !t.superseded_by.is_empty(),
                        Vec::new(),
                        t.types.clone(),
                        false,
                        true,
                    )
                }
            };
            if retired
                || (is_pending && !pending)
                || (family == "type" && (datatype || enumeration))
            {
                continue;
            }
            items.push(Item {
                name: name.clone(),
                pending: is_pending,
                domains,
                ranges,
                datatype,
                enumeration,
            });
            if items.len() == 41 {
                break;
            }
        }
        let after = if items.len() > 40 {
            items.truncate(40);
            items.last().map(|i| i.name.clone())
        } else {
            None
        };
        Ok(Page {
            release: RELEASE,
            license: "https://creativecommons.org/licenses/by-sa/3.0/",
            items,
            after,
        })
    }
}
pub fn get() -> Result<&'static Vocabulary> {
    static REGISTRY: OnceLock<std::result::Result<Vocabulary, ()>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| Vocabulary::parse().map_err(|_| ()))
        .as_ref()
        .map_err(|_| invalid())
}
