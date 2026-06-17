use crate::app::colors;
use crate::byteedit::ByteEdit;
use crate::selection::Selection;
use crate::ui_search::Search;
use eframe::egui;
use intelhexlib::IntelHex;

// ---------------------------------------------------------------------------
// CellFlags
// ---------------------------------------------------------------------------

/// Per-byte rendering flags computed from selection, search, and editor state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellFlags(u8);

impl CellFlags {
    pub const EMPTY: Self = Self(0);
    pub const SELECTED: Self = Self(0b0000_0001);
    pub const SEARCH_HIT: Self = Self(0b0000_0010);
    pub const MODIFIED: Self = Self(0b0000_0100);
    /// Byte differs from a comparison source (diff view)
    pub const DIFFERENT: Self = Self(0b0000_1000);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

/// Describes which portion of the address space is visible.
pub struct Viewport {
    /// Base address aligned to row boundary (e.g., 0x10, 0x20).
    /// Accounts for address gaps in sparse hex files.
    pub display_start: usize,
    /// First visible row index (relative to `display_start`).
    pub first_row: usize,
    pub row_count: usize,
    pub bytes_per_row: usize,
}

// ---------------------------------------------------------------------------
// VisiblePage
// ---------------------------------------------------------------------------

/// Pre-computed snapshot of everything the renderer needs for one viewport.
/// Built by `PageBuilder`, consumed by `HexRenderer`.
pub struct VisiblePage {
    pub start_addr: usize,
    pub bytes_per_row: usize,
    pub row_count: usize,
    pub data: Vec<Option<u8>>,
    pub flags: Vec<CellFlags>,
    pub ascii: Vec<char>,
    pub display_hex: Vec<String>,
}

// ---------------------------------------------------------------------------
// PageContext
// ---------------------------------------------------------------------------

/// Overlays (selection, search, editor, diff) used when building a [`VisiblePage`].
/// Keeps `PageBuilder` independent of `HexSession` so a diff panel can pass
/// `diff_addrs` while reusing the same compute path.
pub struct PageContext<'a> {
    pub selection: &'a Selection,
    pub editor: &'a ByteEdit,
    pub search: &'a Search,
    /// Addresses to mark [`CellFlags::DIFFERENT`] (e.g. from [`crate::hexdiff::DiffSet`]).
    pub diff_addrs: Option<&'a std::collections::HashSet<usize>>,
}

impl<'a> PageContext<'a> {
    /// Context for the standard single-file hex editor
    #[must_use]
    pub const fn editor(
        selection: &'a Selection,
        editor: &'a ByteEdit,
        search: &'a Search,
    ) -> Self {
        Self {
            selection,
            editor,
            search,
            diff_addrs: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Layout (shared by paint and hit-testing)
// ---------------------------------------------------------------------------

const COLUMN_GAP: f32 = 16.0;
const HEX_GROUP_GAP: f32 = 5.0;
const HEX_GROUP_SIZE: usize = 8;

/// Column geometry shared by painting and hit-testing so both use identical math.
pub(crate) struct HexLayout {
    char_w: f32,
    cell_w: f32,
    bytes_per_row: usize,
}

impl HexLayout {
    fn from_char_w(char_w: f32, bytes_per_row: usize) -> Self {
        Self {
            char_w,
            cell_w: char_w * 3.0,
            bytes_per_row,
        }
    }

    const fn addr_w(&self) -> f32 {
        self.char_w * 8.0
    }

    fn hex_x(&self, origin_x: f32) -> f32 {
        origin_x + self.addr_w() + COLUMN_GAP
    }

    fn hex_col_w(&self) -> f32 {
        self.cell_w * self.bytes_per_row as f32
            + HEX_GROUP_GAP * ((self.bytes_per_row / HEX_GROUP_SIZE).saturating_sub(1)) as f32
    }

    fn ascii_x(&self, origin_x: f32) -> f32 {
        self.hex_x(origin_x) + self.hex_col_w() + COLUMN_GAP
    }

    fn total_width(&self) -> f32 {
        self.ascii_x(0.0) + self.char_w * self.bytes_per_row as f32
    }

    /// X offset of hex cell `col` within a row (accounts for 8-byte group gaps).
    fn hex_cell_x(&self, hex_x: f32, col: usize) -> f32 {
        let group = col / HEX_GROUP_SIZE;
        (group as f32).mul_add(HEX_GROUP_GAP, (col as f32).mul_add(self.cell_w, hex_x))
    }

    /// Map an x coordinate inside the hex column to a byte column index.
    /// Returns `None` for clicks in inter-group gaps or outside the column.
    fn hit_test_hex_col(&self, rel_x: f32, hex_x: f32) -> Option<usize> {
        let hex_col_w = self.hex_col_w();
        if rel_x < hex_x || rel_x >= hex_x + hex_col_w {
            return None;
        }
        for col in 0..self.bytes_per_row {
            let x0 = self.hex_cell_x(hex_x, col);
            if (x0..x0 + self.cell_w).contains(&rel_x) {
                return Some(col);
            }
        }
        None
    }
}

/// Resolve the monospace font from the active egui style (keeps layout in sync with theme).
fn monospace_font(ui: &egui::Ui) -> egui::FontId {
    egui::TextStyle::Monospace.resolve(ui.style())
}

/// Measure monospace character width using the same font as painting.
fn measure_char_width(ui: &egui::Ui, font_id: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap("0".to_string(), font_id.clone(), colors::GRAY_160)
        .rect
        .width()
}

// ---------------------------------------------------------------------------
// PageBuilder
// ---------------------------------------------------------------------------

/// Computes a `VisiblePage` from model state. Caches the search highlight set
/// to avoid rebuilding it every frame.
#[derive(Default)]
pub struct PageBuilder {
    /// Pre-built set of every byte address covered by a search match.
    search_highlights: std::collections::HashSet<usize>,
    /// `Search::results_version` used to build the current cache.
    search_cache_version: u64,
}

impl PageBuilder {
    /// Merge extra flags into an already-built page (addresses outside the viewport are ignored).
    ///
    /// Alternative to `PageContext::diff_addrs` when flags are computed after `compute`.
    pub fn apply_addr_flags(
        page: &mut VisiblePage,
        flags: impl IntoIterator<Item = (usize, CellFlags)>,
    ) {
        let row_len = page.bytes_per_row;
        for (addr, extra) in flags {
            if addr < page.start_addr {
                continue;
            }
            let idx = addr - page.start_addr;
            if idx < row_len * page.row_count {
                page.flags[idx].insert(extra);
            }
        }
    }

    /// Compute a `VisiblePage` for the given viewport and model state.
    ///
    /// The page is written into the provided `out` reference so that the caller
    /// owns the result.
    pub fn compute(
        &mut self,
        ih: &IntelHex,
        viewport: &Viewport,
        ctx: &PageContext<'_>,
        out: &mut VisiblePage,
    ) {
        let len = viewport.row_count * viewport.bytes_per_row;
        let start_addr = viewport.display_start + viewport.first_row * viewport.bytes_per_row;

        // Lazily rebuild search highlight cache
        let search_results = &ctx.search.results;
        let search_length = ctx.search.length;
        if self.search_cache_version != ctx.search.results_version {
            self.search_highlights.clear();
            for &start in search_results {
                for addr in start..start.saturating_add(search_length) {
                    self.search_highlights.insert(addr);
                }
            }
            self.search_cache_version = ctx.search.results_version;
        }

        // Fetch data window
        out.start_addr = start_addr;
        out.bytes_per_row = viewport.bytes_per_row;
        out.row_count = viewport.row_count;

        out.data.clear();
        out.data.extend(ih.iter_range(start_addr, len));

        out.flags.clear();
        out.ascii.clear();
        out.display_hex.clear();

        let sel_range = ctx.selection.get_normalized_range();
        let editor_active = ctx.editor.in_progress;
        let editor_buf = &ctx.editor.buffer;
        let editor_preview_range = editor_active
            .then(|| ctx.editor.addr)
            .flatten()
            .map(|[start, end]| (start.min(end), start.max(end)));

        // Compute per-byte flags, hex display strings, and ASCII chars
        for (i, byte) in out.data.iter().enumerate() {
            let addr = start_addr + i;

            // Flags
            let mut flags = CellFlags::EMPTY;
            if let Some((sel_min, sel_max)) = sel_range
                && addr >= sel_min
                && addr <= sel_max
            {
                flags.insert(CellFlags::SELECTED);
            }
            if self.search_highlights.contains(&addr) {
                flags.insert(CellFlags::SEARCH_HIT);
            }
            if ctx.diff_addrs.is_some_and(|diff| diff.contains(&addr)) {
                flags.insert(CellFlags::DIFFERENT);
            }
            if ctx.editor.modified.contains_key(&addr) {
                flags.insert(CellFlags::MODIFIED);
            }
            out.flags.push(flags);

            // Hex display: show editor buffer on the edit target range while typing.
            #[allow(clippy::option_if_let_else)]
            let display = if let Some(b) = byte {
                if editor_preview_range.is_some_and(|(min, max)| addr >= min && addr <= max) {
                    editor_buf.clone()
                } else {
                    format!("{b:02X}")
                }
            } else {
                "--".to_string()
            };
            out.display_hex.push(display);

            // ASCII
            let ch = byte.map_or(' ', |b| {
                if b.is_ascii_graphic() {
                    b as char
                } else {
                    '\u{00B7}' // middle dot for non-printable
                }
            });
            out.ascii.push(ch);
        }
    }
}

// ---------------------------------------------------------------------------
// HexRenderer
// ---------------------------------------------------------------------------

/// Response from `HexRenderer::paint` describing user interaction with the
/// rendered hex view.
pub struct HexResponse {
    /// The address of the byte the user clicked or dragged on (if any).
    pub interacted_addr: Option<usize>,
}

/// Pre-computed layout metrics for rendering
struct LayoutMetrics {
    /// Full row height: font height + egui's `item_spacing.y` (inter-row gap).
    row_height: f32,
    layout: HexLayout,
    /// Top-left corner of the rendered area (from `allocate_at_least`).
    origin: egui::Pos2,
    font_id: egui::FontId,
}

/// Stateless renderer that paints a `VisiblePage` and performs hit-testing.
///
/// The renderer uses `egui::Painter` for direct drawing (no per-cell widgets).
/// Each byte gets:
/// - An optional background rect (selection, search, modified highlights)
/// - A text label (hex value or ASCII char)
/// - An optional hover stroke border (on top of everything)
///
/// Hit testing converts a screen pixel position back to a byte address using
/// the same fixed-width metrics, so no per-cell widget responses are needed.
pub struct HexRenderer;

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
impl HexRenderer {
    /// Paint the hex view from a pre-computed `VisiblePage`.
    ///
    /// Layout: `[address: 8 hex chars] [gap] [hex bytes] [gap] [ASCII chars]`
    ///
    /// Each hex cell is `3 x char_w` wide (2 hex digits + 1 space).
    /// Every 8 bytes an extra 5px gap is inserted for visual grouping.
    ///
    /// Interaction: the painter draws, then we check the pointer position to
    /// determine which byte (if any) is hovered or being clicked/dragged on.
    /// The caller receives `HexResponse::interacted_addr` to update selection.
    pub fn paint(ui: &mut egui::Ui, page: &VisiblePage) -> HexResponse {
        let font_id = monospace_font(ui);
        let row_height =
            ui.text_style_height(&egui::TextStyle::Monospace) + ui.spacing().item_spacing.y;
        let char_w = measure_char_width(ui, &font_id);
        let layout = HexLayout::from_char_w(char_w, page.bytes_per_row);
        let total_w = layout.total_width();

        // Allocate space in the scroll area's layout. This advances the UI
        // cursor so the scroll area knows the content height.
        let (rect, _response) = ui.allocate_at_least(
            egui::vec2(total_w, row_height * page.row_count as f32),
            egui::Sense::hover(),
        );

        // Determine which byte the pointer is over:
        // We check the pointer position once per frame and map it to a byte
        // address via hit_test (pure geometry, no per-cell widgets).
        //
        // - If the primary mouse button is held down -> it's a click or drag,
        //   so we report the address as `interacted_addr` for selection updates.
        // - If the pointer is hovering (not pressed) -> it's a hover, used to
        //   draw a border around the byte under the cursor.
        let mut hovered_addr = None;
        let mut interacted_addr = None;
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && rect.contains(pos)
        {
            let hit = Self::hit_test(pos, rect.min, page, &layout, row_height);
            if ui.input(|i| i.pointer.primary_down()) {
                interacted_addr = hit;
            } else {
                hovered_addr = hit;
            }
        }

        let m = LayoutMetrics {
            row_height,
            layout,
            origin: rect.min,
            font_id,
        };

        let hover_stroke = egui::Stroke::new(1.0, colors::GRAY_160);
        let painter = ui.painter();

        for r in 0..page.row_count {
            Self::paint_address(painter, page, r, &m);
            Self::paint_hex_row(painter, page, r, &m, hovered_addr, hover_stroke);
            Self::paint_ascii_row(painter, page, r, &m, hovered_addr, hover_stroke);
        }

        HexResponse { interacted_addr }
    }

    /// Paint address row
    fn paint_address(painter: &egui::Painter, page: &VisiblePage, r: usize, m: &LayoutMetrics) {
        let row_y = (r as f32).mul_add(m.row_height, m.origin.y);
        let addr = page.start_addr + r * page.bytes_per_row;
        painter.text(
            egui::pos2(m.origin.x, row_y),
            egui::Align2::LEFT_TOP,
            format!("{addr:08X}"),
            m.font_id.clone(),
            colors::GRAY_160,
        );
    }

    /// Paint one row of hex bytes.
    ///
    /// For each byte in the row:
    /// 1. Compute the cell rect (position + size)
    /// 2. Paint an optional background rect for highlight states
    /// 3. Paint the hex text (e.g. "0A") centered in the cell
    /// 4. Paint an optional hover stroke border on top
    fn paint_hex_row(
        painter: &egui::Painter,
        page: &VisiblePage,
        r: usize,
        m: &LayoutMetrics,
        hovered_addr: Option<usize>,
        hover_stroke: egui::Stroke,
    ) {
        let row_y = (r as f32).mul_add(m.row_height, m.origin.y);
        let addr = page.start_addr + r * page.bytes_per_row;
        let offset = r * page.bytes_per_row;
        let hex_x = m.layout.hex_x(m.origin.x);

        for i in 0..page.bytes_per_row {
            let byte_addr = addr + i;
            let byte_idx = offset + i;
            let flags = page.flags[byte_idx];

            let x = m.layout.hex_cell_x(hex_x, i);
            let cell_rect = egui::Rect::from_min_size(
                egui::pos2(x, row_y),
                egui::vec2(m.layout.cell_w, m.row_height),
            );

            // Background: paint colored rects for special states only.
            // Normal bytes have no background.
            // Priority: selected > search hit > different > modified > (none)
            if flags.contains(CellFlags::SELECTED) {
                painter.rect_filled(cell_rect, 0.0, colors::LIGHT_BLUE);
            } else if flags.contains(CellFlags::SEARCH_HIT) {
                painter.rect_filled(cell_rect, 0.0, colors::GREEN);
            } else if flags.contains(CellFlags::DIFFERENT) {
                painter.rect_filled(cell_rect, 0.0, colors::DIFF);
            } else if flags.contains(CellFlags::MODIFIED) {
                painter.rect_filled(cell_rect, 0.0, colors::MUD);
            }

            // Text color: alternate between lighter (even addr) and darker (odd addr) grays.
            let text_color = if byte_addr.is_multiple_of(2) {
                colors::GRAY_210
            } else {
                colors::GRAY_160
            };

            painter.text(
                cell_rect.center(),
                egui::Align2::CENTER_CENTER,
                &page.display_hex[byte_idx],
                m.font_id.clone(),
                text_color,
            );

            // Hover stroke: painted last so it appears on top of any background
            // (selection / search hit / modified). This gives visual feedback
            // similar to egui's native button hover.
            if hovered_addr == Some(byte_addr) {
                painter.rect_stroke(cell_rect, 0.0, hover_stroke, egui::StrokeKind::Inside);
            }
        }
    }

    /// Paint one row of ASCII characters.
    ///
    /// Similar to the hex row but uses single characters (one `char_w` wide).
    fn paint_ascii_row(
        painter: &egui::Painter,
        page: &VisiblePage,
        r: usize,
        m: &LayoutMetrics,
        hovered_addr: Option<usize>,
        hover_stroke: egui::Stroke,
    ) {
        let row_y = (r as f32).mul_add(m.row_height, m.origin.y);
        let addr = page.start_addr + r * page.bytes_per_row;
        let offset = r * page.bytes_per_row;

        for i in 0..page.bytes_per_row {
            let byte_addr = addr + i;
            let byte_idx = offset + i;
            let ch = page.ascii[byte_idx];
            let flags = page.flags[byte_idx];

            let x = (i as f32).mul_add(m.layout.char_w, m.layout.ascii_x(m.origin.x));
            let cell_rect = egui::Rect::from_min_size(
                egui::pos2(x, row_y),
                egui::vec2(m.layout.char_w, m.row_height),
            );

            if flags.contains(CellFlags::SELECTED) {
                painter.rect_filled(cell_rect, 0.0, colors::LIGHT_BLUE);
            } else if flags.contains(CellFlags::SEARCH_HIT) {
                painter.rect_filled(cell_rect, 0.0, colors::GREEN);
            } else if flags.contains(CellFlags::DIFFERENT) {
                painter.rect_filled(cell_rect, 0.0, colors::DIFF);
            }

            painter.text(
                cell_rect.center(),
                egui::Align2::CENTER_CENTER,
                ch.to_string(),
                m.font_id.clone(),
                colors::GRAY_160,
            );

            if hovered_addr == Some(byte_addr) {
                painter.rect_stroke(cell_rect, 0.0, hover_stroke, egui::StrokeKind::Inside);
            }
        }
    }

    /// Inverse of the paint layout: map a screen position to a byte address.
    /// Uses the same [`HexLayout`] as painting so hit regions stay aligned.
    ///
    /// Returns `None` outside hex/ascii columns, in column gaps, or in 8-byte group gaps.
    pub fn hit_test(
        pos: egui::Pos2,
        origin: egui::Pos2,
        page: &VisiblePage,
        layout: &HexLayout,
        row_height: f32,
    ) -> Option<usize> {
        let rel = pos - origin;
        let row = (rel.y / row_height).floor();
        if row < 0.0 || row >= page.row_count as f32 {
            return None;
        }
        let row = row as usize;

        let hex_x = layout.hex_x(0.0);
        let ascii_x = layout.ascii_x(0.0);
        let rel_x = rel.x;

        if let Some(col) = layout.hit_test_hex_col(rel_x, hex_x) {
            return Some(page.start_addr + row * page.bytes_per_row + col);
        }

        if rel_x >= ascii_x {
            let col = ((rel_x - ascii_x) / layout.char_w).floor();
            if col >= 0.0 && col < page.bytes_per_row as f32 {
                return Some(page.start_addr + row * page.bytes_per_row + col as usize);
            }
        }

        None
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    // clippy::field_reassign_with_default,
    // clippy::doc_markdown,
    // clippy::cast_precision_loss,
    // clippy::suboptimal_flops,
    // clippy::unnecessary_cast,
    // clippy::manual_midpoint
)]
mod tests {
    use super::*;
    use crate::byteedit::ByteEdit;
    use crate::selection::Selection;
    use crate::ui_search::Search;
    use intelhexlib::IntelHex;

    /// Create a 16-byte IntelHex at 0x0000..=0x000F with known values:
    fn make_ih() -> IntelHex {
        let mut ih = IntelHex::new();
        ih.write_range(0x0000, 0x000F).unwrap();
        ih.update_range(
            0x0000,
            &[
                0x41, 0x42, 0x00, 0xFF, 0x30, 0x39, 0x20, 0x7F, 0x80, 0x01, 0x02, 0x03, 0x04, 0x05,
                0x06, 0x07,
            ],
        )
        .unwrap();
        ih
    }

    fn make_page() -> VisiblePage {
        VisiblePage {
            start_addr: 0,
            bytes_per_row: 16,
            row_count: 1,
            data: Vec::new(),
            flags: Vec::new(),
            ascii: Vec::new(),
            display_hex: Vec::new(),
        }
    }

    fn compute_page(
        ih: &IntelHex,
        sel: &Selection,
        editor: &ByteEdit,
        search: &Search,
    ) -> VisiblePage {
        let mut pb = PageBuilder::default();
        let viewport = Viewport {
            display_start: 0,
            first_row: 0,
            row_count: 1,
            bytes_per_row: 16,
        };
        let ctx = PageContext::editor(sel, editor, search);
        let mut page = make_page();
        pb.compute(ih, &viewport, &ctx, &mut page);
        page
    }

    /// Like `compute_page` but passes a `diff_addrs` overlay via `PageContext`.
    fn compute_page_with_diff(
        ih: &IntelHex,
        diff_addrs: &std::collections::HashSet<usize>,
    ) -> VisiblePage {
        let mut pb = PageBuilder::default();
        let viewport = Viewport {
            display_start: 0,
            first_row: 0,
            row_count: 1,
            bytes_per_row: 16,
        };
        let ctx = PageContext {
            selection: &Selection::default(),
            editor: &ByteEdit::default(),
            search: &Search::default(),
            diff_addrs: Some(diff_addrs),
        };
        let mut page = make_page();
        pb.compute(ih, &viewport, &ctx, &mut page);
        page
    }

    // ======================================================================
    // CellFlags tests
    // ======================================================================

    #[test]
    fn cellflags_duplicate_flag_is_ignored() {
        // Arrange
        let mut f = CellFlags::EMPTY;

        // Assert
        assert!(!f.contains(CellFlags::SELECTED));
        assert!(!f.contains(CellFlags::SEARCH_HIT));
        assert!(!f.contains(CellFlags::MODIFIED));

        // Act
        f.insert(CellFlags::SELECTED);
        f.insert(CellFlags::SELECTED);

        // Assert
        assert!(f.contains(CellFlags::SELECTED));
        assert!(!f.contains(CellFlags::SEARCH_HIT));
    }

    #[test]
    fn cellflags_single_flag_contains_itself() {
        assert!(CellFlags::SELECTED.contains(CellFlags::SELECTED));
        assert!(CellFlags::SEARCH_HIT.contains(CellFlags::SEARCH_HIT));
        assert!(CellFlags::MODIFIED.contains(CellFlags::MODIFIED));
    }

    #[test]
    fn cellflags_insert_creates_combo() {
        // Arrange
        let mut f = CellFlags::EMPTY;

        // Act
        f.insert(CellFlags::SELECTED);
        f.insert(CellFlags::SEARCH_HIT);

        // Assert
        assert!(f.contains(CellFlags::SELECTED));
        assert!(f.contains(CellFlags::SEARCH_HIT));
        assert!(!f.contains(CellFlags::MODIFIED));
    }

    // ======================================================================
    // PageBuilder::compute tests
    // ======================================================================

    #[test]
    fn compute_basic_contiguous_data() {
        // Arrange
        let ih = make_ih();

        // Act
        let page = compute_page(
            &ih,
            &Selection::default(),
            &ByteEdit::default(),
            &Search::default(),
        );

        // Assert
        assert_eq!(page.start_addr, 0x0000);
        assert_eq!(page.bytes_per_row, 16);
        assert_eq!(page.row_count, 1);
        assert_eq!(page.data.len(), 16);
        assert_eq!(page.data[0], Some(0x41));
        assert_eq!(page.data[3], Some(0xFF));
        assert_eq!(page.display_hex[0], "41");
        assert_eq!(page.display_hex[3], "FF");
    }

    #[test]
    fn compute_sparse_data_with_gaps() {
        // Arrange
        let mut ih = IntelHex::new();
        ih.write_range(0x0000, 0x0003).unwrap();
        ih.update_range(0x0000, &[0xAA, 0xBB, 0xCC, 0xDD]).unwrap();
        ih.write_range(0x0010, 0x0013).unwrap();
        ih.update_range(0x0010, &[0xEE, 0xFF, 0x11, 0x22]).unwrap();

        let mut pb = PageBuilder::default();
        let viewport = Viewport {
            display_start: 0,
            first_row: 0,
            row_count: 2,
            bytes_per_row: 16,
        };
        let mut page = VisiblePage {
            start_addr: 0,
            bytes_per_row: 16,
            row_count: 2,
            data: Vec::new(),
            flags: Vec::new(),
            ascii: Vec::new(),
            display_hex: Vec::new(),
        };

        // Act
        pb.compute(
            &ih,
            &viewport,
            &PageContext::editor(
                &Selection::default(),
                &ByteEdit::default(),
                &Search::default(),
            ),
            &mut page,
        );

        // Assert
        // First row: 4 bytes of data, 12 gaps
        assert_eq!(page.data[0], Some(0xAA));
        assert_eq!(page.data[3], Some(0xDD));
        assert_eq!(page.data[4], None); // gap
        assert_eq!(page.data[15], None); // gap
        assert_eq!(page.display_hex[0], "AA");
        assert_eq!(page.display_hex[4], "--");
        // Second row: data at 0x10..0x13
        assert_eq!(page.data[16], Some(0xEE));
        assert_eq!(page.data[17], Some(0xFF));
        assert_eq!(page.data[18], Some(0x11));
        assert_eq!(page.data[19], Some(0x22));
        // Remaining bytes in row 2 are gaps
        assert_eq!(page.data[20], None);
        assert_eq!(page.data[31], None);
    }

    #[test]
    fn compute_ascii_printable_vs_nonprintable() {
        // Arrange
        let ih = make_ih();

        // Act
        let page = compute_page(
            &ih,
            &Selection::default(),
            &ByteEdit::default(),
            &Search::default(),
        );

        // Assert
        assert_eq!(page.ascii[0], 'A'); // 0x41
        assert_eq!(page.ascii[1], 'B'); // 0x42
        assert_eq!(page.ascii[2], '\u{00B7}'); // 0x00 -> middle dot
        assert_eq!(page.ascii[3], '\u{00B7}'); // 0xFF -> middle dot
        assert_eq!(page.ascii[4], '0'); // 0x30
        assert_eq!(page.ascii[5], '9'); // 0x39
    }

    #[test]
    fn compute_selection_flags() {
        // Arrange
        let ih = make_ih();
        let mut sel = Selection::default();
        sel.range = Some([0x03, 0x06]);
        sel.released = true;

        // Act
        let page = compute_page(&ih, &sel, &ByteEdit::default(), &Search::default());

        // Assert
        assert!(!page.flags[0].contains(CellFlags::SELECTED));
        assert!(!page.flags[2].contains(CellFlags::SELECTED));
        assert!(page.flags[3].contains(CellFlags::SELECTED));
        assert!(page.flags[4].contains(CellFlags::SELECTED));
        assert!(page.flags[5].contains(CellFlags::SELECTED));
        assert!(page.flags[6].contains(CellFlags::SELECTED));
        assert!(!page.flags[7].contains(CellFlags::SELECTED));
    }

    #[test]
    fn compute_search_hit_flags() {
        // Arrange
        let ih = make_ih();
        let mut search = Search::default();
        search.replace_results(vec![0x02, 0x08], 3);

        // Act
        let page = compute_page(&ih, &Selection::default(), &ByteEdit::default(), &search);

        // Assert
        // 0x02..0x04
        assert!(!page.flags[1].contains(CellFlags::SEARCH_HIT));
        assert!(page.flags[2].contains(CellFlags::SEARCH_HIT));
        assert!(page.flags[3].contains(CellFlags::SEARCH_HIT));
        assert!(page.flags[4].contains(CellFlags::SEARCH_HIT));
        assert!(!page.flags[5].contains(CellFlags::SEARCH_HIT));
        // 0x08..0x0A
        assert!(page.flags[8].contains(CellFlags::SEARCH_HIT));
        assert!(page.flags[9].contains(CellFlags::SEARCH_HIT));
        assert!(page.flags[10].contains(CellFlags::SEARCH_HIT));
        assert!(!page.flags[11].contains(CellFlags::SEARCH_HIT));
    }

    #[test]
    fn compute_modified_flags() {
        // Arrange
        let ih = make_ih();
        let mut editor = ByteEdit::default();
        editor.modified.insert(0x05, 0x00);
        editor.modified.insert(0x0A, 0x00);

        // Act
        let page = compute_page(&ih, &Selection::default(), &editor, &Search::default());

        // Assert
        assert!(!page.flags[4].contains(CellFlags::MODIFIED));
        assert!(page.flags[5].contains(CellFlags::MODIFIED));
        assert!(!page.flags[9].contains(CellFlags::MODIFIED));
        assert!(page.flags[10].contains(CellFlags::MODIFIED));
    }

    #[test]
    fn compute_combined_flags() {
        // Arrange
        let ih = make_ih();

        let mut sel = Selection::default();
        sel.range = Some([0x02, 0x04]);
        sel.released = true;

        let mut search = Search::default();
        search.replace_results(vec![0x02], 2);

        // Act
        let page = compute_page(&ih, &sel, &ByteEdit::default(), &search);

        // Assert
        // 0x02: both SELECTED and SEARCH_HIT
        assert!(page.flags[2].contains(CellFlags::SELECTED));
        assert!(page.flags[2].contains(CellFlags::SEARCH_HIT));
        // 0x03: both
        assert!(page.flags[3].contains(CellFlags::SELECTED));
        assert!(page.flags[3].contains(CellFlags::SEARCH_HIT));
        // 0x04: SELECTED only
        assert!(page.flags[4].contains(CellFlags::SELECTED));
        assert!(!page.flags[4].contains(CellFlags::SEARCH_HIT));
    }

    #[test]
    fn compute_editor_buffer_override() {
        // Arrange
        let ih = make_ih();

        let mut sel = Selection::default();
        sel.range = Some([0x02, 0x02]);
        sel.released = true;

        let mut editor = ByteEdit::default();
        editor.in_progress = true;
        editor.buffer = "AB".to_string();
        editor.addr = Some([0x02, 0x02]);

        // Act
        let page = compute_page(&ih, &sel, &editor, &Search::default());

        // Assert
        // Selected byte shows editor buffer
        assert_eq!(page.display_hex[2], "AB");
        // Non-selected byte shows actual value
        assert_eq!(page.display_hex[0], "41");
    }

    #[test]
    fn compute_editor_buffer_on_all_selected() {
        // Arrange
        let ih = make_ih();

        let mut sel = Selection::default();
        sel.range = Some([0x02, 0x05]);
        sel.released = true;

        let mut editor = ByteEdit::default();
        editor.in_progress = true;
        editor.buffer = "F".to_string(); // 1 char typed
        editor.addr = Some([0x02, 0x05]);

        // Act
        let page = compute_page(&ih, &sel, &editor, &Search::default());

        // Assert
        // All selected bytes show the editor buffer (preview)
        assert_eq!(page.display_hex[2], "F");
        assert_eq!(page.display_hex[3], "F");
        assert_eq!(page.display_hex[4], "F");
        assert_eq!(page.display_hex[5], "F");
        // Non-selected byte shows actual value
        assert_eq!(page.display_hex[0], "41");
    }

    #[test]
    fn compute_editor_preview_uses_addr_when_selection_cleared() {
        // Arrange - edit in progress but selection cleared (e.g., side-panel focus)
        let ih = make_ih();
        let sel = Selection::default();

        let mut editor = ByteEdit::default();
        editor.in_progress = true;
        editor.buffer = "A".to_string();
        editor.addr = Some([0x02, 0x04]);

        let mut pb = PageBuilder::default();
        let viewport = Viewport {
            display_start: 0,
            first_row: 0,
            row_count: 1,
            bytes_per_row: 16,
        };
        let search = Search::default();
        let ctx = PageContext::editor(&sel, &editor, &search);
        let mut page = make_page();

        // Act
        pb.compute(&ih, &viewport, &ctx, &mut page);

        // Assert — preview on edit range, not on selection
        assert_eq!(page.display_hex[2], "A");
        assert_eq!(page.display_hex[3], "A");
        assert_eq!(page.display_hex[4], "A");
        assert_eq!(page.display_hex[0], "41");
    }

    #[test]
    fn compute_reversed_selection() {
        // Arrange
        let ih = make_ih();

        let mut sel = Selection::default();
        sel.range = Some([0x06, 0x03]); // reversed: head before anchor
        sel.released = true;

        // Act
        let page = compute_page(&ih, &sel, &ByteEdit::default(), &Search::default());

        // Assert
        assert!(page.flags[3].contains(CellFlags::SELECTED));
        assert!(page.flags[4].contains(CellFlags::SELECTED));
        assert!(page.flags[5].contains(CellFlags::SELECTED));
        assert!(page.flags[6].contains(CellFlags::SELECTED));
        assert!(!page.flags[2].contains(CellFlags::SELECTED));
        assert!(!page.flags[7].contains(CellFlags::SELECTED));
    }

    #[test]
    fn compute_search_cache_invalidation() {
        // Arrange
        let ih = make_ih();

        let mut pb = PageBuilder::default();
        let viewport = Viewport {
            display_start: 0,
            first_row: 0,
            row_count: 1,
            bytes_per_row: 16,
        };
        let mut page = make_page();

        // First call: no search results
        let mut search = Search::default();

        // Act
        pb.compute(
            &ih,
            &viewport,
            &PageContext::editor(&Selection::default(), &ByteEdit::default(), &search),
            &mut page,
        );

        // Assert
        assert!(!page.flags[5].contains(CellFlags::SEARCH_HIT));

        // Arrange
        // Second call: search results added
        search.replace_results(vec![0x05], 1);

        // Act
        pb.compute(
            &ih,
            &viewport,
            &PageContext::editor(&Selection::default(), &ByteEdit::default(), &search),
            &mut page,
        );

        // Assert
        assert!(page.flags[5].contains(CellFlags::SEARCH_HIT));

        // Arrange
        // Third call: different result, same length (cache must invalidate)
        search.replace_results(vec![0x06], 1);

        // Act
        pb.compute(
            &ih,
            &viewport,
            &PageContext::editor(&Selection::default(), &ByteEdit::default(), &search),
            &mut page,
        );

        // Assert
        assert!(!page.flags[5].contains(CellFlags::SEARCH_HIT));
        assert!(page.flags[6].contains(CellFlags::SEARCH_HIT));

        // Arrange
        // Fourth call: search results cleared
        search.replace_results(Vec::new(), 0);

        // Act
        pb.compute(
            &ih,
            &viewport,
            &PageContext::editor(&Selection::default(), &ByteEdit::default(), &search),
            &mut page,
        );

        // Assert
        assert!(!page.flags[5].contains(CellFlags::SEARCH_HIT));
    }

    #[test]
    fn compute_diff_flags() {
        // Arrange
        let ih = make_ih();
        let mut diff = std::collections::HashSet::new();
        diff.insert(0x04);
        diff.insert(0x05);

        // Act
        let page = compute_page_with_diff(&ih, &diff);

        // Assert
        assert!(!page.flags[3].contains(CellFlags::DIFFERENT));
        assert!(page.flags[4].contains(CellFlags::DIFFERENT));
        assert!(page.flags[5].contains(CellFlags::DIFFERENT));
        assert!(!page.flags[6].contains(CellFlags::DIFFERENT));
    }

    #[test]
    fn apply_addr_flags_marks_in_viewport_only() {
        // Arrange
        let ih = make_ih();
        let mut page = compute_page(
            &ih,
            &Selection::default(),
            &ByteEdit::default(),
            &Search::default(),
        );

        // Act
        PageBuilder::apply_addr_flags(&mut page, [(0x02, CellFlags::DIFFERENT)]);

        // Assert
        assert!(page.flags[2].contains(CellFlags::DIFFERENT));
        assert!(!page.flags[0].contains(CellFlags::DIFFERENT));
    }

    // ======================================================================
    // HexRenderer::hit_test tests
    // ======================================================================

    /// Helper: create a minimal VisiblePage for hit_test (only metadata matters)
    fn make_hit_page(start_addr: usize, bytes_per_row: usize, row_count: usize) -> VisiblePage {
        VisiblePage {
            start_addr,
            bytes_per_row,
            row_count,
            data: vec![None; bytes_per_row * row_count],
            flags: vec![CellFlags::EMPTY; bytes_per_row * row_count],
            ascii: vec![' '; bytes_per_row * row_count],
            display_hex: vec!["--".to_string(); bytes_per_row * row_count],
        }
    }

    /// Helper: layout used by hit_test tests.
    fn hit_layout(page: &VisiblePage, char_w: f32) -> (egui::Pos2, f32, HexLayout) {
        let origin = egui::pos2(100.0, 50.0);
        let layout = HexLayout::from_char_w(char_w, page.bytes_per_row);
        (origin, 19.0, layout)
    }

    fn hex_cell_center(
        origin: egui::Pos2,
        layout: &HexLayout,
        row_h: f32,
        row: usize,
        col: usize,
    ) -> egui::Pos2 {
        // Center of hex cell at (row, col) - mirrors paint layout.
        let row_y = origin.y + row as f32 * row_h;
        let hex_x = layout.hex_x(origin.x);
        let x = layout.hex_cell_x(hex_x, col) + layout.cell_w / 2.0;
        egui::pos2(x, row_y + row_h / 2.0)
    }

    #[test]
    fn hit_test_first_hex_cell() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x1000, 16, 4);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let pos = hex_cell_center(origin, &layout, row_h, 0, 0);

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, Some(0x1000));
    }

    #[test]
    fn hit_test_last_hex_cell_in_row() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x1000, 16, 4);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let pos = hex_cell_center(origin, &layout, row_h, 0, 15);

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, Some(0x100F));
    }

    #[test]
    fn hit_test_byte_in_second_group() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x0000, 16, 1);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let pos = hex_cell_center(origin, &layout, row_h, 0, 8);

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, Some(0x0008));
    }

    #[test]
    fn hit_test_byte_nine_near_byte_eight_edge() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x0000, 16, 1);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        // Near the right edge of byte 8's cell — must not map to byte 9.
        let pos = hex_cell_center(origin, &layout, row_h, 0, 8);
        let mut near_right = pos;
        near_right.x = layout.hex_cell_x(layout.hex_x(origin.x), 8) + layout.cell_w - 1.0;

        // Act
        let res = HexRenderer::hit_test(near_right, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, Some(0x0008));
    }

    #[test]
    fn hit_test_first_ascii_cell() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x1000, 16, 4);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let ascii_x = layout.ascii_x(origin.x);
        let pos = egui::pos2(ascii_x + layout.char_w / 2.0, origin.y + row_h / 2.0);

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, Some(0x1000));
    }

    #[test]
    fn hit_test_second_row() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x1000, 16, 4);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let pos = hex_cell_center(origin, &layout, row_h, 1, 0);

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, Some(0x1010));
    }

    #[test]
    fn hit_test_above_view_returns_none() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x1000, 16, 4);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let mut pos = hex_cell_center(origin, &layout, row_h, 0, 0);
        pos.y = origin.y - 1.0;

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, None);
    }

    #[test]
    fn hit_test_below_last_row_returns_none() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x1000, 16, 4);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let mut pos = hex_cell_center(origin, &layout, row_h, 0, 0);
        pos.y = origin.y + row_h * 4.0 + 1.0;

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, None);
    }

    #[test]
    fn hit_test_in_address_column_returns_none() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x1000, 16, 4);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let pos = egui::pos2(origin.x + 10.0, origin.y + row_h / 2.0);

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, None);
    }

    #[test]
    fn hit_test_in_gap_between_columns_returns_none() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x1000, 16, 4);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let hex_x = layout.hex_x(origin.x);
        let hex_end = hex_x + layout.hex_col_w();
        let ascii_x = layout.ascii_x(origin.x);
        let gap_center = (hex_end + ascii_x) / 2.0;
        let pos = egui::pos2(gap_center, origin.y + row_h / 2.0);

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, None);
    }

    #[test]
    fn hit_test_in_inter_group_gap_returns_none() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x0000, 16, 1);
        let (origin, row_h, layout) = hit_layout(&page, char_w);
        let hex_x = layout.hex_x(origin.x);
        // 5px gap between bytes 7 and 8
        let pos = egui::pos2(hex_x + 8.0 * layout.cell_w + 2.0, origin.y + row_h / 2.0);

        // Act
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, None);
    }

    #[test]
    fn hit_test_nonzero_start_addr() {
        // Arrange
        let char_w = 7.2;
        let page = make_hit_page(0x2000, 16, 2);
        let (origin, row_h, layout) = hit_layout(&page, char_w);

        // Act - first hex cell
        let pos = hex_cell_center(origin, &layout, row_h, 0, 0);
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, Some(0x2000));

        // Act - first ASCII cell, second row
        let ascii_x = layout.ascii_x(origin.x);
        let pos = egui::pos2(ascii_x + layout.char_w / 2.0, origin.y + row_h * 1.5);
        let res = HexRenderer::hit_test(pos, origin, &page, &layout, row_h);

        // Assert
        assert_eq!(res, Some(0x2010));
    }
}
