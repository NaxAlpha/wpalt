//! Real owner-local font admission and bounded malformed-container refusal.
use wpalt::theme::font;
const FONT: &[u8] = include_bytes!("fixtures/fonts/Aboreto-Regular.ttf");

#[test]
fn real_font_admission_checks_tables_glyph_work_and_corruption() {
    let inspected = font::inspect(FONT).expect("Unmodified licensed reference font is admitted");
    assert_eq!(inspected.bytes, 48356);
    assert_eq!(
        inspected.sha256,
        "fc702d535857fa2bda802647b9d55f82a07502d4acdfd95b47279b3bf479eaa1"
    );
    assert!(inspected.glyphs > 100 && inspected.units_per_em >= 16);
    for length in [0, 11, 64, FONT.len() / 2] {
        assert!(
            font::inspect(&FONT[..length]).is_err(),
            "Truncated font ({length} bytes) must fail"
        );
    }
    let mut corrupt = FONT.to_vec();
    let table_offset = u32::from_be_bytes(corrupt[20..24].try_into().unwrap()) as usize;
    corrupt[table_offset] ^= 1;
    assert!(
        font::inspect(&corrupt).is_err(),
        "A changed table cannot pass the original checksum"
    );
    let mut directory = FONT.to_vec();
    directory[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
    assert!(
        font::inspect(&directory).is_err(),
        "Unbounded table claims fail before parsing"
    );
    let mut overlap = FONT.to_vec();
    overlap[20..24].copy_from_slice(&12u32.to_be_bytes());
    assert!(
        font::inspect(&overlap).is_err(),
        "Tables cannot alias the directory"
    );
    // Preserve table checksums while introducing a composite self-cycle: this
    // exercises graph admission rather than failing a superficial byte check.
    use read_fonts::{FontRef, TableProvider};
    let parsed = FontRef::new(FONT).unwrap();
    let loca = parsed.loca(None).unwrap();
    let table_count = u16::from_be_bytes(FONT[4..6].try_into().unwrap()) as usize;
    let glyf_record = (0..table_count)
        .map(|i| 12 + i * 16)
        .find(|i| &FONT[*i..*i + 4] == b"glyf")
        .unwrap();
    let start =
        u32::from_be_bytes(FONT[glyf_record + 8..glyf_record + 12].try_into().unwrap()) as usize;
    let length =
        u32::from_be_bytes(FONT[glyf_record + 12..glyf_record + 16].try_into().unwrap()) as usize;
    let composite = (0..usize::from(inspected.glyphs))
        .find_map(|id| {
            let offset = start + loca.get_raw(id)? as usize;
            let end = start + loca.get_raw(id + 1)? as usize;
            (end > offset && FONT.get(offset..offset + 2) == Some(&[255, 255]))
                .then_some((id, offset))
        })
        .expect("Fixture has meaningful composite glyphs");
    let mut cycle = FONT.to_vec();
    cycle[composite.1 + 12..composite.1 + 14].copy_from_slice(&(composite.0 as u16).to_be_bytes());
    let sum = cycle[start..start + length]
        .chunks(4)
        .fold(0u32, |sum, part| {
            let mut value = [0; 4];
            value[..part.len()].copy_from_slice(part);
            sum.wrapping_add(u32::from_be_bytes(value))
        });
    cycle[glyf_record + 4..glyf_record + 8].copy_from_slice(&sum.to_be_bytes());
    assert!(
        font::inspect(&cycle).is_err(),
        "A checksummed composite self-cycle is refused without rasterization"
    );
    let mut disguised = FONT.to_vec();
    disguised[..4].copy_from_slice(b"wOF2");
    assert!(
        font::inspect(&disguised).is_err(),
        "Unsupported containers are not admitted by a filename"
    );
    assert!(
        font::inspect(&vec![0; font::MAX_FONT_BYTES + 1]).is_err(),
        "Encoded input has an explicit byte ceiling"
    );
}
