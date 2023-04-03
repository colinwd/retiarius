use crate::Datagram;

pub trait Filter {
    fn apply(&self, input: Datagram) -> Option<Datagram>;
}

pub mod drop_chance;