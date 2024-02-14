mod cache;

use bytes::{Buf, BufMut};
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("0.0.0.0:5353").await.unwrap();

    loop {
        let mut buf = vec![0; 1500];
        let (len, addr) = socket.recv_from(&mut buf).await.unwrap();
        buf.truncate(len);
        println!("{:?}", addr);

        let packet = Packet::decode(&buf[..]).unwrap();
        dbg!(&packet);
        dbg!(
            packet.header.qr(),
            packet.header.opcode(),
            packet.header.aa(),
            packet.header.tc(),
            packet.header.ra(),
            packet.header.rd(),
            packet.header.rcode()
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct Header {
    pub transaction_id: u16,
    pub flags: u16,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Qr {
    Request,
    Response,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpCode {
    Query,
    InverseQuery,
    Status,
}

impl Header {
    pub fn qr(&self) -> Qr {
        match self.flags >> 15 {
            0 => Qr::Request,
            1 => Qr::Response,
            _ => unreachable!(),
        }
    }

    pub fn opcode(&self) -> OpCode {
        match (self.flags & 0b0111_1000_0000_0000) >> 11 {
            0 => OpCode::Query,
            1 => OpCode::InverseQuery,
            2 => OpCode::Status,
            //Reserved
            _ => todo!(),
        }
    }

    pub fn aa(&self) -> bool {
        match (self.flags & 0b0000_0100_0000_0000) >> 10 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    pub fn tc(&self) -> bool {
        match (self.flags & 0b0000_0010_0000_0000) >> 9 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    pub fn rd(&self) -> bool {
        match (self.flags & 0b0000_0001_0000_0000) >> 8 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    pub fn ra(&self) -> bool {
        match (self.flags & 0b0000_0000_1000_0000) >> 7 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    pub fn rcode(&self) -> ResponseCode {
        let tag = self.flags & 0b0000_0000_0000_1111;
        ResponseCode::from_u16(tag).unwrap()
    }

    pub fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        buf.put_u16(self.transaction_id);
        buf.put_u16(self.flags);
        buf.put_u16(self.qdcount);
        buf.put_u16(self.ancount);
        buf.put_u16(self.nscount);
        buf.put_u16(self.arcount);
    }

    pub fn decode<B>(mut buf: B) -> Result<Self, ()>
    where
        B: Buf,
    {
        if buf.remaining() < 12 {
            return Err(());
        }

        Ok(Self {
            transaction_id: buf.get_u16(),
            flags: buf.get_u16(),
            qdcount: buf.get_u16(),
            ancount: buf.get_u16(),
            nscount: buf.get_u16(),
            arcount: buf.get_u16(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub header: Header,
    pub questions: Vec<Question>,
}

impl Packet {
    pub fn decode<B>(mut buf: B) -> Result<Self, ()>
    where
        B: Buf,
    {
        let header = Header::decode(&mut buf)?;

        let mut questions = Vec::new();
        for _ in 0..header.qdcount {
            questions.push(Question::decode(&mut buf)?);
        }

        Ok(Self { header, questions })
    }
}

#[derive(Clone, Debug)]
pub struct Question {
    pub name: Fqdn,
    pub qtype: Type,
    pub qclass: Class,
}

impl Question {
    pub fn decode<B>(mut buf: B) -> Result<Self, ()>
    where
        B: Buf,
    {
        let name = Fqdn::decode(&mut buf)?;
        let qtype = Type::from_u16(buf.get_u16()).unwrap();
        let qclass = Class::from_u16(buf.get_u16()).unwrap();
        Ok(Self {
            name,
            qclass,
            qtype,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Fqdn(String);

impl Fqdn {
    fn decode<B>(mut buf: B) -> Result<Self, ()>
    where
        B: Buf,
    {
        let mut fqdn = String::new();
        loop {
            let len = buf.get_u8();
            if len & 0b1100_0000 != 0 {
                todo!()
            }

            if len == 0 {
                return Ok(Self(fqdn));
            }

            for _ in 0..len {
                let v = buf.get_u8();
                fqdn.push_str(std::str::from_utf8(&[v]).unwrap());
            }

            fqdn.push_str(".");
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    A,
    NS,
    MD,
    MF,
    CNAME,
    SOA,
    MB,
    MG,
    MR,
    NULL,
    WKS,
    PTR,
    HINFO,
    MINFO,
    MX,
    TXT,
}

macro_rules! enum_as_int {
    ($id:ident, $($int:tt => $val:tt),*,) => {
        impl $id {
            pub fn from_u16(tag: u16) -> Option<Self> {
                match tag {
                    $(
                        $int => Some(Self::$val),
                    )*
                    _ => None,
                }
            }

            pub fn to_u16(self) -> u16 {
                match self {
                    $(
                        Self::$val => $int,
                    )*
                }
            }

        }
    };
}

enum_as_int! {
    Type,
    1 => A,
    2 => NS,
    3 => MD,
    4 => MF,
    5 => CNAME,
    6 => SOA,
    7 => MB,
    8 => MG,
    9 => MR,
    10 => NULL,
    11 => WKS,
    12 => PTR,
    13 => HINFO,
    14 => MINFO,
    15 => MX,
    16 => TXT,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    In,
}

enum_as_int! {
    Class,
    1 => In,
}

#[derive(Clone, Debug)]
pub struct ResourceRecord {
    name: Fqdn,
    r#type: Type,
    class: Class,
    ttl: u32,
    rdlength: u16,
    rddata: Vec<u8>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResponseCode {
    Ok,
    FormatError,
    ServerFailure,
    NameError,
    NotImplemented,
    Refused,
}

enum_as_int! {
    ResponseCode,
    0 => Ok,
    1 => FormatError,
    2 => ServerFailure,
    3 => NameError,
    4 => NotImplemented,
    5 => Refused,
}
