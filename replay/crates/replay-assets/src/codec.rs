pub struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8], magic: &[u8; 4]) -> Option<Reader<'a>> {
        bytes.strip_prefix(magic).map(Reader)
    }

    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk()?;
        self.0 = rest;
        Some(*head)
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.take().map(u8::from_le_bytes)
    }

    pub fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_le_bytes)
    }

    pub fn i16(&mut self) -> Option<i16> {
        self.take().map(i16::from_le_bytes)
    }

    pub fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }

    pub fn list<T>(&mut self, mut read: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let len = self.u32()?;
        (0..len).map(|_| read(self)).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn write_list<I>(out: &mut Vec<u8>, items: I, write: impl Fn(&mut Vec<u8>, I::Item))
where
    I: IntoIterator<IntoIter: ExactSizeIterator>,
{
    let items = items.into_iter();
    out.extend((items.len() as u32).to_le_bytes());
    for item in items {
        write(out, item);
    }
}
