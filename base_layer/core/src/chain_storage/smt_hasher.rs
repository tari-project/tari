use jmt::SimpleHasher;

pub(crate) struct SmtHasher {}

impl SimpleHasher for SmtHasher {
    fn new() -> Self {
        todo!()
    }

    fn update(&mut self, data: &[u8]) {
        todo!()
    }

    fn finalize(self) -> [u8; 32] {
        todo!()
    }
}
