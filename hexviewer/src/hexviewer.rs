use super::edit::Editor;
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
    /// IntelHex object returned by intelhex library
    pub ih: IntelHex,
    /// Address-to-byte map
    pub byte_addr_map: BTreeMap<usize, u8>,
    /// Byte edit logic/handler
    pub editor: Editor,
    /// Smallest address of the hex data
    pub min_addr: usize,
    /// Largest address of the hex data
    pub max_addr: usize,
    /// Which bytes are currently being selected
    pub selection: Selection,
    /// Endianness of the hex data
    pub endianness: Endianness,
    /// Error during intelhex parsing
    pub error: Option<String>,
}
