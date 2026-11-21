pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn rest(self) -> &'a [u8] {
        self.bytes
    }

    pub fn bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.bytes.split_at_checked(len)?;
        self.bytes = tail;
        Some(head)
    }

    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.bytes(N)?.try_into().ok()
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.take().map(u8::from_be_bytes)
    }

    pub fn i8(&mut self) -> Option<i8> {
        self.take().map(i8::from_be_bytes)
    }

    pub fn bool(&mut self) -> Option<bool> {
        self.u8().map(|byte| byte != 0)
    }

    pub fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_be_bytes)
    }

    pub fn i16(&mut self) -> Option<i16> {
        self.take().map(i16::from_be_bytes)
    }

    pub fn i32(&mut self) -> Option<i32> {
        self.take().map(i32::from_be_bytes)
    }

    pub fn i64(&mut self) -> Option<i64> {
        self.take().map(i64::from_be_bytes)
    }

    pub fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_be_bytes)
    }

    pub fn f64(&mut self) -> Option<f64> {
        self.take().map(f64::from_be_bytes)
    }

    pub fn uuid(&mut self) -> Option<u128> {
        self.take().map(u128::from_be_bytes)
    }

    pub fn varint(&mut self) -> Option<i32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.u8()?;
            value |= u32::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Some(value as i32);
            }
        }
        None
    }

    pub fn varint_len(&mut self) -> Option<usize> {
        usize::try_from(self.varint()?).ok()
    }

    pub fn string(&mut self) -> Option<String> {
        let len = self.varint_len()?;
        Some(String::from_utf8_lossy(self.bytes(len)?).into_owned())
    }

    pub fn position(&mut self) -> Option<[i32; 3]> {
        let packed = self.i64()?;
        Some([packed >> 38, packed << 26 >> 52, packed << 38 >> 38].map(|axis| axis as i32))
    }

    pub fn triple<T>(&mut self, mut read: impl FnMut(&mut Self) -> Option<T>) -> Option<[T; 3]> {
        Some([read(self)?, read(self)?, read(self)?])
    }

    pub fn optional<T>(&mut self, read: impl FnOnce(&mut Self) -> Option<T>) -> Option<Option<T>> {
        if self.bool()? {
            read(self).map(Some)
        } else {
            Some(None)
        }
    }

    pub fn array<T>(
        &mut self,
        len: usize,
        mut read: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<Vec<T>> {
        (0..len).map(|_| read(self)).collect()
    }

    pub fn list<T>(&mut self, read: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let len = self.varint_len()?;
        self.array(len, read)
    }
}
