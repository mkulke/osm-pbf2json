extern crate osm_pbf2json;

use osm_pbf2json::{filter, process};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};

fn get_string(cursor: &mut Cursor<Vec<u8>>) -> String {
    cursor.seek(SeekFrom::Start(0)).unwrap();
    let mut out = Vec::new();
    cursor.read_to_end(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn find_fountains_or_townhalls() {
    let mut cursor = Cursor::new(Vec::new());
    let groups = filter::parse("amenity~fountain+tourism,amenity~townhall".to_string());
    let file = File::open("./tests/data/alexanderplatz.pbf").unwrap();
    process(file, &mut cursor, &groups).unwrap();

    let string = get_string(&mut cursor);
    let lines: Vec<&str> = string.trim().split('\n').collect();
    assert_eq!(lines.len(), 4);
    for line in lines {
        assert!(
            (line.contains(r#"amenity":"fountain"#) && line.contains(r#"tourism"#))
                || line.contains(r#"amenity":"townhall"#)
        );
    }
}

#[test]
fn find_bike_parking_for_six() {
    let mut cursor = Cursor::new(Vec::new());
    let groups = filter::parse("amenity~bicycle_parking+capacity~6".to_string());
    let file = File::open("./tests/data/alexanderplatz.pbf").unwrap();
    process(file, &mut cursor, &groups).unwrap();

    let string = get_string(&mut cursor);
    let lines: Vec<&str> = string.trim().split('\n').collect();
    assert_eq!(lines.len(), 14);
}
