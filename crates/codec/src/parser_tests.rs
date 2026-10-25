use super::AccessUnitParser;
use crate::VideoCodec;

const TINY_H264: &[u8] = include_bytes!("../tests/data/tiny.h264");
const TINY_HEVC: &[u8] = include_bytes!("../tests/data/tiny.hevc");

/// Feeds `data` in `chunk`-byte pieces, then flushes; returns the length of every AU.
fn split(parser: &mut AccessUnitParser, data: &[u8], chunk: usize) -> Vec<usize> {
    let mut lens = Vec::new();
    for piece in data.chunks(chunk) {
        let mut input = piece;
        while !input.is_empty() {
            if let Some(au) = parser.push(&mut input).unwrap() {
                assert!(!au.is_empty());
                lens.push(au.len());
            }
        }
    }
    while let Some(au) = parser.flush().unwrap() {
        lens.push(au.len());
    }
    lens
}

#[test]
fn splits_tiny_h264_into_10_aus() {
    let mut parser = AccessUnitParser::new(VideoCodec::H264).unwrap();
    let lens = split(&mut parser, TINY_H264, 1000);
    assert_eq!(lens.len(), 10);
    assert_eq!(lens.iter().sum::<usize>(), TINY_H264.len());
}

#[test]
fn splits_tiny_hevc_into_10_aus() {
    let mut parser = AccessUnitParser::new(VideoCodec::Hevc).unwrap();
    let lens = split(&mut parser, TINY_HEVC, 1000);
    assert_eq!(lens.len(), 10);
    assert_eq!(lens.iter().sum::<usize>(), TINY_HEVC.len());
}

#[test]
fn chunk_size_does_not_matter() {
    let mut big = AccessUnitParser::new(VideoCodec::H264).unwrap();
    let mut tiny = AccessUnitParser::new(VideoCodec::H264).unwrap();
    assert_eq!(
        split(&mut tiny, TINY_H264, 1),
        split(&mut big, TINY_H264, TINY_H264.len())
    );
}

#[test]
fn reset_restarts_cleanly() {
    let mut fresh = AccessUnitParser::new(VideoCodec::H264).unwrap();
    let expected = split(&mut fresh, TINY_H264, 1000);

    let mut parser = AccessUnitParser::new(VideoCodec::H264).unwrap();
    let mut half = &TINY_H264[..TINY_H264.len() / 2];
    while !half.is_empty() {
        let _ = parser.push(&mut half).unwrap();
    }
    parser.reset().unwrap();
    assert_eq!(split(&mut parser, TINY_H264, 1000), expected);
}

#[test]
fn truncated_tail_is_returned_by_flush() {
    let mut fresh = AccessUnitParser::new(VideoCodec::H264).unwrap();
    let full = split(&mut fresh, TINY_H264, 1000);

    let cut = &TINY_H264[..TINY_H264.len() - 100];
    let mut parser = AccessUnitParser::new(VideoCodec::H264).unwrap();
    let lens = split(&mut parser, cut, 1000);
    assert_eq!(lens.len(), 10);
    assert_eq!(lens[9], full[9] - 100);
}

#[test]
fn from_extension_maps_names() {
    let cases = [
        ("h264", Some(VideoCodec::H264)),
        ("264", Some(VideoCodec::H264)),
        ("H264", Some(VideoCodec::H264)),
        ("hevc", Some(VideoCodec::Hevc)),
        ("h265", Some(VideoCodec::Hevc)),
        ("265", Some(VideoCodec::Hevc)),
        ("HEVC", Some(VideoCodec::Hevc)),
        ("mp4", None),
        ("", None),
    ];
    for (ext, expected) in cases {
        assert_eq!(VideoCodec::from_extension(ext), expected, "{ext}");
    }
}
