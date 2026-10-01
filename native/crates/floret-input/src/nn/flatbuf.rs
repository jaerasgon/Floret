//! Just enough FlatBuffers reading for TFLite model files.
//!
//! Every read is bounds-checked and returns `Err` on a malformed buffer, so
//! a corrupt model can't panic the tracker thread.

pub type R<T> = Result<T, String>;

fn bytes<const N: usize>(buf: &[u8], pos: usize) -> R<[u8; N]> {
    buf.get(pos..pos + N)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| format!("model truncated at byte {pos}"))
}

fn u16_at(buf: &[u8], pos: usize) -> R<u16> {
    bytes(buf, pos).map(u16::from_le_bytes)
}

fn u32_at(buf: &[u8], pos: usize) -> R<u32> {
    bytes(buf, pos).map(u32::from_le_bytes)
}

/// A table: an offset into the buffer plus its vtable.
#[derive(Clone, Copy)]
pub struct Table<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Table<'a> {
    pub fn root(buf: &'a [u8]) -> R<Table<'a>> {
        let pos = u32_at(buf, 0)? as usize;
        Ok(Table { buf, pos })
    }

    /// Absolute position of field `id`, `None` when absent.
    fn field(&self, id: usize) -> R<Option<usize>> {
        let soff = i32::from_le_bytes(bytes(self.buf, self.pos)?);
        let vt = (self.pos as i64 - i64::from(soff)) as usize;
        let vt_len = usize::from(u16_at(self.buf, vt)?);
        let slot = 4 + 2 * id;
        if slot + 2 > vt_len {
            return Ok(None);
        }
        let off = usize::from(u16_at(self.buf, vt + slot)?);
        Ok((off != 0).then_some(self.pos + off))
    }

    fn deref(&self, id: usize) -> R<Option<usize>> {
        match self.field(id)? {
            Some(p) => Ok(Some(p + u32_at(self.buf, p)? as usize)),
            None => Ok(None),
        }
    }

    pub fn u8(&self, id: usize, default: u8) -> R<u8> {
        self.field(id)?
            .map_or(Ok(default), |p| bytes::<1>(self.buf, p).map(|b| b[0]))
    }

    pub fn bool(&self, id: usize) -> R<bool> {
        Ok(self.u8(id, 0)? != 0)
    }

    pub fn i32(&self, id: usize, default: i32) -> R<i32> {
        self.field(id)?
            .map_or(Ok(default), |p| bytes(self.buf, p).map(i32::from_le_bytes))
    }

    pub fn u32(&self, id: usize, default: u32) -> R<u32> {
        self.field(id)?.map_or(Ok(default), |p| u32_at(self.buf, p))
    }

    pub fn u64(&self, id: usize, default: u64) -> R<u64> {
        self.field(id)?
            .map_or(Ok(default), |p| bytes(self.buf, p).map(u64::from_le_bytes))
    }

    pub fn table(&self, id: usize) -> R<Option<Table<'a>>> {
        Ok(self.deref(id)?.map(|pos| Table { buf: self.buf, pos }))
    }

    pub fn vector(&self, id: usize) -> R<Option<Vector<'a>>> {
        match self.deref(id)? {
            Some(p) => Ok(Some(Vector {
                buf: self.buf,
                start: p + 4,
                len: u32_at(self.buf, p)? as usize,
            })),
            None => Ok(None),
        }
    }

    pub fn str(&self, id: usize) -> R<&'a str> {
        match self.vector(id)? {
            Some(v) => std::str::from_utf8(v.bytes(1)?).map_err(|e| e.to_string()),
            None => Ok(""),
        }
    }

    /// An `[int]` field (empty when absent).
    pub fn i32s(&self, id: usize) -> R<Vec<i32>> {
        match self.vector(id)? {
            Some(v) => (0..v.len).map(|i| v.i32(i)).collect(),
            None => Ok(Vec::new()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Vector<'a> {
    buf: &'a [u8],
    start: usize,
    pub len: usize,
}

impl<'a> Vector<'a> {
    /// The raw element bytes, for elements `elem` bytes wide.
    pub fn bytes(&self, elem: usize) -> R<&'a [u8]> {
        self.buf
            .get(self.start..self.start + self.len * elem)
            .ok_or_else(|| "vector out of bounds".to_owned())
    }

    pub fn i32(&self, i: usize) -> R<i32> {
        bytes(self.buf, self.start + 4 * i).map(i32::from_le_bytes)
    }

    pub fn u16(&self, i: usize) -> R<u16> {
        u16_at(self.buf, self.start + 2 * i)
    }

    pub fn u8(&self, i: usize) -> R<u8> {
        bytes::<1>(self.buf, self.start + i).map(|b| b[0])
    }

    pub fn table(&self, i: usize) -> R<Table<'a>> {
        let p = self.start + 4 * i;
        Ok(Table {
            buf: self.buf,
            pos: p + u32_at(self.buf, p)? as usize,
        })
    }
}
