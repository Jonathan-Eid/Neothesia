//! Dumps what a score was converted into, useful when debugging MusicXML files.
//!
//! `cargo run -p midi-file --example musicxml_info -- score.mxl`

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: musicxml_info <file.mxl|file.musicxml|file.mid>");
        std::process::exit(1);
    };

    let file = midi_file::MidiFile::new(&path).unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });

    let end = file
        .tracks
        .iter()
        .filter_map(|track| track.notes.last())
        .map(|note| note.end)
        .max()
        .unwrap_or_default();

    println!("{}", file.name);
    println!("  measures: {}", file.measures.len());
    println!("  length: {:.1}s", end.as_secs_f32());

    for track in file.tracks.iter() {
        if track.notes.is_empty() {
            continue;
        }

        let lowest = track.notes.iter().map(|n| n.note).min().unwrap_or(0);
        let highest = track.notes.iter().map(|n| n.note).max().unwrap_or(0);
        let programs: Vec<u8> = track.programs.iter().map(|p| p.program).collect();

        println!(
            "  track {} {:?}: {} notes, keys {lowest}..{highest}, programs {programs:?}",
            track.track_id,
            track.name.as_deref().unwrap_or("-"),
            track.notes.len(),
        );
    }
}
