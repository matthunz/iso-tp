#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Single,
    First,
    Consecutive,
    Flow,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowKind {
    Continue,
    Wait,
    Abort,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    bytes: [u8; 8],
}

impl Frame {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut buf = [0; 8];
        for (a, b) in bytes.iter().zip(&mut buf) {
            *b = *a;
        }

        Self { bytes: buf }
    }

    pub fn single(data: &[u8]) -> Option<Self> {
        if data.len() > 7 {
            return None;
        }

        let mut bytes = [0; 8];
        bytes[0] = ((Kind::Single as u8) << 4) | data.len() as u8;
        for (a, b) in data.iter().zip(&mut bytes[1..]) {
            *b = *a;
        }

        Some(Self { bytes })
    }

    pub fn first(data: &[u8]) -> (Self, usize) {
        let mut bytes = [0; 8];
        bytes[0] = ((Kind::First as u8) << 4) | (data.len() & 0b1111) as u8;
        bytes[1] = (data.len() >> 4) as u8;

        let mut used = 0;
        for (a, b) in data.iter().zip(&mut bytes[2..]) {
            *b = *a;
            used += 1;
        }

        (Self { bytes }, used)
    }

    pub fn consecutive(index: u8, data: &[u8]) -> (Self, usize) {
        let mut bytes = [0; 8];
        bytes[0] = ((Kind::Consecutive as u8) << 4) | index;

        let mut used = 0;
        for (a, b) in data.iter().zip(&mut bytes[1..]) {
            *b = *a;
            used += 1;
        }

        (Self { bytes }, used)
    }

    pub fn flow(kind: FlowKind, block_len: u8, st: u8) -> Self {
        let mut bytes = [0; 8];
        bytes[0] = ((Kind::Flow as u8) << 4) | kind as u8;
        bytes[1] = block_len;
        bytes[2] = st;

        Self { bytes }
    }

    pub fn kind(&self) -> Option<Kind> {
        let kind = match (self.bytes[0] >> 4) & 0b00001111 {
            0 => Kind::Single,
            1 => Kind::First,
            2 => Kind::Consecutive,
            3 => Kind::Flow,
            _ => return None,
        };

        Some(kind)
    }

    pub fn single_data(&self) -> &[u8] {
        let len = self.bytes[0] & 0b1111;
        &self.bytes[1..=len as usize]
    }

    pub fn consecutive_data(&self) -> &[u8] {
      
        &self.bytes[1..]
    }

    pub fn first_data(&self) -> &[u8] {
        &self.bytes[2..]
    }

    pub fn first_len(&self) -> u16 {
        ((self.bytes[0] & 0b1111) as u16) | ((self.bytes[1] as u16) << 4)
    }

    pub fn flow_kind(&self) -> FlowKind {
        match self.bytes[0] & 011 {
            0 => FlowKind::Continue,
            1 => FlowKind::Wait,
            2 => FlowKind::Abort,
            _ => todo!(),
        }
    }

    pub fn flow_len(&self) -> u8 {
        self.bytes[1]
    }
}

impl AsRef<[u8]> for Frame {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}
