//! MusicXML support.
//!
//! Both the uncompressed (`.musicxml`, `.xml`) and the compressed zip container
//! (`.mxl`) flavours are accepted. The score is converted into an in memory
//! [`midly::Smf`], so that the rest of the crate can treat it exactly like a
//! regular midi file.

mod convert;

use midly::Smf;

pub use convert::PULSES_PER_QUARTER_NOTE;

/// Result of a MusicXML conversion.
pub struct ConvertedScore {
    pub smf: Smf<'static>,
    /// Start of every measure, in pulses.
    pub measures: Vec<u64>,
    /// Name of every track, in the order they appear in `smf`, without the
    /// leading tempo track.
    pub track_names: Vec<Option<String>>,
}

/// Cheap sniffing, so that we don't have to rely on the file extension.
pub fn looks_like_musicxml(data: &[u8]) -> bool {
    if is_zip(data) {
        return true;
    }

    let head = &data[..data.len().min(4096)];
    let head = String::from_utf8_lossy(head);
    head.contains("score-partwise") || head.contains("score-timewise")
}

fn is_zip(data: &[u8]) -> bool {
    data.starts_with(b"PK\x03\x04")
}

pub fn parse(data: &[u8]) -> Result<ConvertedScore, String> {
    let xml = extract_musicxml(data)?;

    let opt = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(&xml, opt)
        .map_err(|err| format!("MusicXML Parsing Error ({err})"))?;

    convert::convert(&doc)
}

/// The raw MusicXML text a `.mxl`/`.musicxml` file contains, decompressed and
/// decoded but otherwise untouched. Useful for tools that want to hand the
/// actual score to something else (an LLM, a diff, a search) rather than the
/// parsed-and-converted result `parse()` produces.
pub fn extract_musicxml(data: &[u8]) -> Result<String, String> {
    if is_zip(data) {
        read_container(data)
    } else {
        decode_text(data.to_vec())
    }
}

/// Pulls the root score out of a `.mxl` zip container.
fn read_container(data: &[u8]) -> Result<String, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))
        .map_err(|err| format!("Could Not Open MXL Container ({err})"))?;

    let names: Vec<String> = (0..zip.len())
        .filter_map(|id| Some(zip.by_index(id).ok()?.name().to_string()))
        .collect();

    // Some writers store paths with windows separators.
    let normalized = |name: &str| name.replace('\\', "/");
    let entry = |path: &str| {
        let path = normalized(path);
        names.iter().find(|name| normalized(name) == path).cloned()
    };

    // `META-INF/container.xml` points at the score, but it is not always there,
    // in which case we fall back to the first plausible entry.
    let root_path = entry("META-INF/container.xml")
        .and_then(|name| read_zip_entry(&mut zip, &name).ok())
        .and_then(|meta| {
            let doc = roxmltree::Document::parse(&meta).ok()?;
            let path = doc
                .descendants()
                .find(|n| n.has_tag_name("rootfile"))?
                .attribute("full-path")?;
            entry(path)
        });

    if let Some(root_path) = root_path
        && let Ok(xml) = read_zip_entry(&mut zip, &root_path)
    {
        return Ok(xml);
    }

    let fallback = names.iter().find(|name| {
        let name = normalized(name).to_ascii_lowercase();
        !name.starts_with("meta-inf/") && (name.ends_with(".xml") || name.ends_with(".musicxml"))
    });

    match fallback.cloned() {
        Some(name) => read_zip_entry(&mut zip, &name),
        None => Err(String::from("MXL Container Has No Score")),
    }
}

fn read_zip_entry(
    zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
) -> Result<String, String> {
    use std::io::Read;

    let mut file = zip
        .by_name(name)
        .map_err(|_| format!("MXL Container Has No `{name}`"))?;

    let mut buff = Vec::new();
    file.read_to_end(&mut buff)
        .map_err(|err| format!("Could Not Read `{name}` ({err})"))?;

    decode_text(buff)
}

/// MusicXML is utf-8 in practice, but utf-16 is legal, and some editors do emit it.
fn decode_text(buff: Vec<u8>) -> Result<String, String> {
    let (utf16_le, utf16_be) = (
        buff.starts_with(&[0xFF, 0xFE]),
        buff.starts_with(&[0xFE, 0xFF]),
    );

    if utf16_le || utf16_be {
        let units: Vec<u16> = buff[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if utf16_le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();

        return String::from_utf16(&units).map_err(|_| String::from("Invalid utf-16 In MusicXML"));
    }

    // Strip the utf-8 bom if present, xml parsers dislike it.
    let buff = match buff.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => rest.to_vec(),
        None => buff,
    };

    String::from_utf8(buff).map_err(|_| String::from("Invalid utf-8 In MusicXML"))
}
