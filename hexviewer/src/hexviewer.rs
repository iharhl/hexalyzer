use std::collections::BTreeMap;
use intelhex::IntelHex;


#[derive(Default)]
pub struct HexViewer {
    pub(crate) ih: IntelHex,
    pub(crate) byte_addr_map: BTreeMap<usize, u8>,
    pub(crate) min_addr: usize,
    pub(crate) max_addr: usize,
    pub(crate) selected: Option<(usize, u8)>,
    // pub(crate) selection: String,
    pub(crate) error: Option<String>,
}
