//! A minimal MP4 writer for one H.264 video track: samples stream straight
//! to the output (one chunk), and the index (`moov`) follows them at the
//! end. Readers that need the index first (web "fast start") have to seek
//! for it, which every desktop player and editor does.
//!
//! The `mdat` header is written as an 8-byte `free` box plus an 8-byte
//! `mdat` box; past 4 GiB of samples the 16 bytes become one 64-bit
//! `mdat` header instead (ffmpeg's trick), so the samples never move.

use std::io::{Seek, SeekFrom, Write};

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 8);
    b.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

/// A full box: version 0 and `flags`.
fn full(kind: &[u8; 4], flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = flags.to_be_bytes().to_vec();
    b.extend_from_slice(body);
    bx(kind, &b)
}

fn u32s(vals: &[u32]) -> Vec<u8> {
    vals.iter().flat_map(|v| v.to_be_bytes()).collect()
}

const MATRIX: [u32; 9] = [0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x4000_0000];
const FTYP: &[u8] = b"isom\0\0\x02\0isomiso2avc1mp41";
/// Track ticks per frame (the track timescale is `fps` × this).
const TICKS: u32 = 1000;

/// Writes an H.264 MP4: [`Mp4Muxer::push`] each frame's sample (AVCC:
/// length-prefixed NAL units, no parameter sets), then
/// [`Mp4Muxer::finish`] with the stream's SPS / PPS.
pub struct Mp4Muxer<W: Write + Seek> {
    out: W,
    width: u32,
    height: u32,
    fps: u32,
    /// Where the `free` + `mdat` headers start.
    mdat_at: u64,
    /// Sample bytes written so far.
    written: u64,
    sizes: Vec<u32>,
    sync: Vec<bool>,
}

impl<W: Write + Seek> Mp4Muxer<W> {
    pub fn new(mut out: W, width: u32, height: u32, fps: u32) -> std::io::Result<Self> {
        let start = out.stream_position()?;
        out.write_all(&bx(b"ftyp", FTYP))?;
        let mdat_at = start + 8 + FTYP.len() as u64;
        out.write_all(&bx(b"free", &[]))?;
        out.write_all(&bx(b"mdat", &[]))?;
        Ok(Mp4Muxer {
            out,
            width,
            height,
            fps: fps.max(1),
            mdat_at,
            written: 0,
            sizes: Vec::new(),
            sync: Vec::new(),
        })
    }

    /// Frames written so far.
    pub fn frames(&self) -> usize {
        self.sizes.len()
    }

    /// Append one frame. `sync`: it's a keyframe (IDR).
    pub fn push(&mut self, sample: &[u8], sync: bool) -> std::io::Result<()> {
        let size = u32::try_from(sample.len())
            .map_err(|_| std::io::Error::other("video frame over 4 GiB"))?;
        self.out.write_all(sample)?;
        self.written += u64::from(size);
        self.sizes.push(size);
        self.sync.push(sync);
        Ok(())
    }

    /// Write the index and fix up the `mdat` size. Returns the output.
    pub fn finish(mut self, sps: &[u8], pps: &[u8]) -> std::io::Result<W> {
        if sps.len() < 4 || pps.is_empty() {
            return Err(std::io::Error::other("no H.264 parameter sets"));
        }
        let moov = self.moov(sps, pps);
        self.out.write_all(&moov)?;
        let end = self.out.stream_position()?;
        let mdat = 8 + self.written;
        self.out.seek(SeekFrom::Start(self.mdat_at))?;
        match u32::try_from(mdat) {
            // free (8) stays; the mdat header after it gets its size.
            Ok(size) => {
                self.out.seek(SeekFrom::Current(8))?;
                self.out.write_all(&size.to_be_bytes())?;
            }
            // One 16-byte header with a 64-bit size over both boxes.
            Err(_) => {
                self.out.write_all(&1u32.to_be_bytes())?;
                self.out.write_all(b"mdat")?;
                self.out.write_all(&(16 + self.written).to_be_bytes())?;
            }
        }
        self.out.seek(SeekFrom::Start(end))?;
        self.out.flush()?;
        Ok(self.out)
    }

    fn moov(&self, sps: &[u8], pps: &[u8]) -> Vec<u8> {
        let timescale = self.fps * TICKS;
        let n = self.sizes.len() as u32;
        let duration = n * TICKS;
        let duration_ms = (u64::from(n) * 1000 / u64::from(self.fps)) as u32;
        let data_offset = self.mdat_at + 16;

        let mut mvhd = u32s(&[0, 0, 1000, duration_ms, 0x10000]);
        mvhd.extend_from_slice(&[1, 0, 0, 0]); // volume 1.0, reserved
        mvhd.extend_from_slice(&[0; 8]);
        mvhd.extend(u32s(&MATRIX));
        mvhd.extend_from_slice(&[0; 24]);
        mvhd.extend(u32s(&[2])); // next track id
        let mvhd = full(b"mvhd", 0, &mvhd);

        let mut tkhd = u32s(&[0, 0, 1, 0, duration_ms, 0, 0]);
        tkhd.extend_from_slice(&[0; 8]); // layer, group, volume, reserved
        tkhd.extend(u32s(&MATRIX));
        tkhd.extend(u32s(&[self.width << 16, self.height << 16]));
        let tkhd = full(b"tkhd", 3, &tkhd);

        let mut mdhd = u32s(&[0, 0, timescale, duration]);
        mdhd.extend_from_slice(&[0x55, 0xc4, 0, 0]); // "und"
        let mdhd = full(b"mdhd", 0, &mdhd);
        let mut hdlr = u32s(&[0]);
        hdlr.extend_from_slice(b"vide");
        hdlr.extend_from_slice(&[0; 12]);
        hdlr.extend_from_slice(b"VideoHandler\0");
        let hdlr = full(b"hdlr", 0, &hdlr);

        let vmhd = full(b"vmhd", 1, &[0; 8]);
        let dref = full(b"dref", 0, &[u32s(&[1]), full(b"url ", 1, &[])].concat());
        let dinf = bx(b"dinf", &dref);

        let mut avcc = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
        avcc.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        avcc.extend_from_slice(sps);
        avcc.push(1);
        avcc.extend_from_slice(&(pps.len() as u16).to_be_bytes());
        avcc.extend_from_slice(pps);
        let mut avc1 = vec![0; 6];
        avc1.extend_from_slice(&[0, 1]); // data reference index
        avc1.extend_from_slice(&[0; 16]);
        avc1.extend_from_slice(&(self.width as u16).to_be_bytes());
        avc1.extend_from_slice(&(self.height as u16).to_be_bytes());
        avc1.extend(u32s(&[0x48_0000, 0x48_0000, 0]));
        avc1.extend_from_slice(&[0, 1]); // frame count
        avc1.extend_from_slice(&[0; 32]); // compressor name
        avc1.extend_from_slice(&[0, 0x18, 0xff, 0xff]); // depth, pre-defined
        avc1.extend(bx(b"avcC", &avcc));
        let stsd = full(b"stsd", 0, &[u32s(&[1]), bx(b"avc1", &avc1)].concat());

        let stts = full(b"stts", 0, &u32s(&[1, n, TICKS]));
        let keys: Vec<u32> = (1..=n).filter(|&i| self.sync[i as usize - 1]).collect();
        let stss = full(
            b"stss",
            0,
            &[u32s(&[keys.len() as u32]), u32s(&keys)].concat(),
        );
        let stsc = full(b"stsc", 0, &u32s(&[1, 1, n, 1]));
        let stsz = full(b"stsz", 0, &[u32s(&[0, n]), u32s(&self.sizes)].concat());
        let stco = match u32::try_from(data_offset) {
            Ok(o) => full(b"stco", 0, &u32s(&[1, o])),
            Err(_) => {
                let mut b = u32s(&[1]);
                b.extend_from_slice(&data_offset.to_be_bytes());
                full(b"co64", 0, &b)
            }
        };
        let stbl = bx(b"stbl", &[stsd, stts, stss, stsc, stsz, stco].concat());

        let minf = bx(b"minf", &[vmhd, dinf, stbl].concat());
        let mdia = bx(b"mdia", &[mdhd, hdlr, minf].concat());
        let trak = bx(b"trak", &[tkhd, mdia].concat());
        bx(b"moov", &[mvhd, trak].concat())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Top-level boxes: (kind, offset, size).
    fn boxes(b: &[u8]) -> Vec<(String, usize, u64)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at + 8 <= b.len() {
            let mut size = u64::from(u32::from_be_bytes(b[at..at + 4].try_into().unwrap()));
            if size == 1 {
                size = u64::from_be_bytes(b[at + 8..at + 16].try_into().unwrap());
            }
            out.push((
                String::from_utf8_lossy(&b[at + 4..at + 8]).into_owned(),
                at,
                size,
            ));
            at += size as usize;
        }
        out
    }

    #[test]
    fn samples_sit_in_mdat_and_the_index_follows() {
        let mut m = Mp4Muxer::new(Cursor::new(Vec::new()), 16, 8, 30).unwrap();
        m.push(&[0, 0, 0, 2, 0x65, 1], true).unwrap();
        m.push(&[0, 0, 0, 1, 0x41], false).unwrap();
        assert_eq!(m.frames(), 2);
        let sps = [0x67, 0x42, 0xc0, 0x1e, 9];
        let b = m.finish(&sps, &[0x68, 1]).unwrap().into_inner();
        let kinds: Vec<_> = boxes(&b).into_iter().map(|(k, _, s)| (k, s)).collect();
        assert_eq!(
            kinds.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["ftyp", "free", "mdat", "moov"]
        );
        assert_eq!(kinds[2].1, 8 + 11, "mdat holds both samples");
        let (_, at, _) = boxes(&b)[2].clone();
        assert_eq!(
            &b[at + 8..at + 19],
            &[0, 0, 0, 2, 0x65, 1, 0, 0, 0, 1, 0x41]
        );
    }

    #[test]
    fn needs_parameter_sets() {
        let m = Mp4Muxer::new(Cursor::new(Vec::new()), 16, 8, 30).unwrap();
        assert!(m.finish(&[], &[]).is_err());
    }
}
