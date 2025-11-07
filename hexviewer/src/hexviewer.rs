use super::selection::Selection;
use intelhex::IntelHex;
use std::collections::BTreeMap;

#[derive(Default, PartialEq)]
pub enum Endianness {
    #[default]
    Little,
    Big,
}

#[derive(Default)]
pub struct HexViewer {
    pub ih: IntelHex,
    pub byte_addr_map: BTreeMap<usize, u8>,
    pub min_addr: usize,
    pub max_addr: usize,
    pub selected: Selection,
    pub endianness: Endianness,
    pub error: Option<String>,
}
