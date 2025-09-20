use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Row, Table, Widget},
};

use super::scroll_state::ScrollState;

pub(crate) struct GenericDisplayRow {
    pub name: String,
    pub match_indices: Option<Vec<usize>>, // 文字位置インデックス
    pub is_current: bool,
    pub description: Option<String>,
}

pub(crate) fn render_rows(
    area: Rect,
    buf: &mut Buffer,
    rows_all: &[GenericDisplayRow],
    state: &ScrollState,
    max_results: usize,
    _dim_non_selected: bool,
) {
    let mut rows: Vec<Row> = Vec::new();
    if rows_all.is_empty() {
        rows.push(Row::new(vec![Cell::from(Line::from(Span::styled(
            "no matches",
            Style::default().add_modifier(Modifier::ITALIC | Modifier::DIM),
        )))]));
    } else {
        let max_rows_from_area = area.height as usize;
        let visible_rows = max_results
            .min(rows_all.len())
            .min(max_rows_from_area.max(1));
        let mut start_idx = state.scroll_top.min(rows_all.len().saturating_sub(1));
        if let Some(sel) = state.selected_idx {
            if sel < start_idx {
                start_idx = sel;
            } else if visible_rows > 0 {
                let bottom = start_idx + visible_rows - 1;
                if sel > bottom {
                    start_idx = sel + 1 - visible_rows;
                }
            }
        }
        for (i, row) in rows_all
            .iter()
            .enumerate()
            .skip(start_idx)
            .take(visible_rows)
        {
            let mut spans: Vec<Span> = Vec::new();
            if let Some(idxs) = row.match_indices.as_ref() {
                let mut it = idxs.iter().peekable();
                for (ci, ch) in row.name.chars().enumerate() {
                    let mut style = Style::default();
                    if it.peek().is_some_and(|n| **n == ci) {
                        it.next();
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    spans.push(Span::styled(ch.to_string(), style));
                }
            } else {
                spans.push(Span::raw(row.name.clone()));
            }
            if let Some(desc) = row.description.as_ref() {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    desc.clone(),
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
            let mut cell = Cell::from(Line::from(spans));
            if Some(i) == state.selected_idx {
                cell = cell.style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            }
            rows.push(Row::new(vec![cell]));
        }
    }

    let table = Table::new(rows, vec![Constraint::Percentage(100)])
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_type(BorderType::QuadrantOutside)
                .border_style(Style::default().add_modifier(Modifier::DIM)),
        )
        .widths([Constraint::Percentage(100)]);
    table.render(area, buf);
}
