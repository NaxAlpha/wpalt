//! Owner-local static TrueType admission. No shaping, bytecode or rasterization runs here.
use crate::error::{Error, Result};
use read_fonts::{FontRef, TableProvider};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_FONT_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inspection {
    pub italic: bool,
    pub glyphs: u16,
    pub units_per_em: u16,
    pub weight: u16,
    pub bytes: usize,
    pub sha256: String,
}
fn invalid() -> Error {
    Error::invalid(
        "Choose a structurally valid static TrueType font within the supported byte/table/glyph budgets.",
    )
}
fn word(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn long(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn checksum(bytes: &[u8], head: bool) -> u32 {
    bytes.chunks(4).enumerate().fold(0u32, |sum, (i, part)| {
        let mut word = [0; 4];
        word[..part.len()].copy_from_slice(part);
        sum.wrapping_add(if head && i == 2 {
            0
        } else {
            u32::from_be_bytes(word)
        })
    })
}
pub fn inspect(bytes: &[u8]) -> Result<Inspection> {
    if bytes.len() < 12 || bytes.len() > MAX_FONT_BYTES || long(bytes, 0)? != 0x00010000 {
        return Err(invalid());
    }
    let count = usize::from(word(bytes, 4)?);
    if !(1..=128).contains(&count) {
        return Err(invalid());
    }
    let directory_end = 12 + count * 16;
    if directory_end > bytes.len() {
        return Err(invalid());
    }
    let mut tables = BTreeMap::new();
    let mut ranges = Vec::new();
    for index in 0..count {
        let offset = 12 + index * 16;
        let tag = &bytes[offset..offset + 4];
        let start = long(bytes, offset + 8)? as usize;
        let length = long(bytes, offset + 12)? as usize;
        let end = start.checked_add(length).ok_or_else(invalid)?;
        if start < directory_end
            || !start.is_multiple_of(4)
            || end > bytes.len()
            || tables.contains_key(tag)
        {
            return Err(invalid());
        }
        let data = &bytes[start..end];
        if checksum(data, tag == b"head") != long(bytes, offset + 4)? {
            return Err(invalid());
        }
        tables.insert(tag, data);
        if length > 0 {
            ranges.push((start, end));
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0)
        || [
            b"fvar".as_slice(),
            b"CFF ",
            b"CFF2",
            b"SVG ",
            b"COLR",
            b"CBDT",
            b"sbix",
        ]
        .iter()
        .any(|tag| tables.contains_key(*tag))
    {
        return Err(invalid());
    }
    let font = FontRef::new(bytes).map_err(|_| invalid())?;
    let head = font.head().map_err(|_| invalid())?;
    let glyphs = font.maxp().map_err(|_| invalid())?.num_glyphs();
    if glyphs == 0
        || !(16..=16384).contains(&head.units_per_em())
        || ![0, 1].contains(&head.index_to_loc_format())
    {
        return Err(invalid());
    }
    font.hhea().map_err(|_| invalid())?;
    font.hmtx().map_err(|_| invalid())?;
    font.name().map_err(|_| invalid())?;
    font.cmap().map_err(|_| invalid())?;
    let weight = font.os2().map_err(|_| invalid())?.us_weight_class();
    if !(1..=1000).contains(&weight) {
        return Err(invalid());
    }
    let loca = font.loca(None).map_err(|_| invalid())?;
    let parsed_glyf = font.glyf().map_err(|_| invalid())?;
    let glyf = tables.get(b"glyf".as_slice()).ok_or_else(invalid)?;
    if !loca.all_offsets_are_ascending()
        || loca
            .get_raw(usize::from(glyphs))
            .is_none_or(|end| end as usize > glyf.len())
    {
        return Err(invalid());
    }
    let mut graph = vec![Vec::new(); usize::from(glyphs)];
    let mut edges = 0;
    let mut points = 0;
    for (index, children) in graph.iter_mut().enumerate() {
        let start = loca.get_raw(index).ok_or_else(invalid)? as usize;
        let end = loca.get_raw(index + 1).ok_or_else(invalid)? as usize;
        if start == end {
            continue;
        }
        let data = glyf.get(start..end).ok_or_else(invalid)?;
        if data.len() < 10 {
            return Err(invalid());
        }
        let contours = word(data, 0)? as i16;
        if contours >= 0 {
            let parsed = loca
                .get(read_fonts::types::GlyphId::new(index as u32), &parsed_glyf)
                .ok_or_else(invalid)?;
            if let Some(read_fonts::tables::glyf::Glyph::Simple(simple)) = parsed.into_glyph() {
                points += simple.num_points();
                if points > 1_000_000 || simple.points().count() != simple.num_points() {
                    return Err(invalid());
                }
            } else {
                return Err(invalid());
            }
        } else {
            if contours != -1 {
                return Err(invalid());
            }
            let mut offset = 10;
            let last_flags = loop {
                let flags = word(data, offset)?;
                let child = usize::from(word(data, offset + 2)?);
                if child >= usize::from(glyphs) || children.len() >= 64 {
                    return Err(invalid());
                }
                children.push(child);
                edges += 1;
                if edges > 131072 {
                    return Err(invalid());
                }
                let transforms = usize::from(flags & 8 != 0)
                    + usize::from(flags & 64 != 0)
                    + usize::from(flags & 128 != 0);
                if transforms > 1 {
                    return Err(invalid());
                }
                offset += 4
                    + if flags & 1 != 0 { 4 } else { 2 }
                    + if flags & 8 != 0 {
                        2
                    } else if flags & 64 != 0 {
                        4
                    } else if flags & 128 != 0 {
                        8
                    } else {
                        0
                    };
                if offset > data.len() {
                    return Err(invalid());
                }
                if flags & 32 == 0 {
                    break flags;
                }
            };
            if last_flags & 256 != 0 {
                let length = usize::from(word(data, offset)?);
                offset += 2 + length;
            }
            if offset > data.len()
                || data.len() - offset > 3
                || data[offset..].iter().any(|b| *b != 0)
            {
                return Err(invalid());
            }
        }
    }
    fn visit(
        node: usize,
        graph: &[Vec<usize>],
        state: &mut [u8],
        height: &mut [u8],
        depth: usize,
    ) -> Result<u8> {
        if depth > 32 || state[node] == 1 {
            return Err(invalid());
        }
        if state[node] == 2 {
            return Ok(height[node]);
        }
        state[node] = 1;
        let mut result = 1;
        for &child in &graph[node] {
            result = result.max(visit(child, graph, state, height, depth + 1)? + 1);
        }
        if result > 32 {
            return Err(invalid());
        }
        state[node] = 2;
        height[node] = result;
        Ok(result)
    }
    let mut state = vec![0; graph.len()];
    let mut height = vec![0; graph.len()];
    for index in 0..graph.len() {
        visit(index, &graph, &mut state, &mut height, 1)?;
    }
    Ok(Inspection {
        italic: font
            .os2()
            .map_err(|_| invalid())?
            .fs_selection()
            .contains(read_fonts::tables::os2::SelectionFlags::ITALIC),
        glyphs,
        units_per_em: head.units_per_em(),
        weight,
        bytes: bytes.len(),
        sha256: crate::auth::digest(bytes),
    })
}
