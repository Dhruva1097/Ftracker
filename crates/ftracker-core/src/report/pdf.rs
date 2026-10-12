use anyhow::Result;
use chrono::{Local, TimeZone};
use printpdf::{
    BuiltinFont, Image, ImageTransform, IndirectFontRef, Mm, PdfDocument, PdfLayerIndex,
    PdfLayerReference, PdfPageIndex,
};
use std::io::BufWriter;

use crate::config::AppConfig;
use crate::domain::time::format_hm;
use crate::report::day_totals::{fmt_loc_opt, required_seconds, MonthDayRow};
use crate::Store;
use crate::VERSION;

pub fn build_daily_pdf(store: &Store, cfg: &AppConfig, day: &str) -> Result<Vec<u8>> {
    let required = required_seconds(cfg.daily_hours_required);
    let counted = store.day_counted(day).unwrap_or(0);
    let idle = store.day_idle(day).unwrap_or(0);
    let cats = store.category_seconds(day).unwrap_or_default();
    let (loc_first, loc_last, fixes) = store.first_last_loc_day(day).unwrap_or((None, None, 0));
    let shots = store.screenshots_day(day).unwrap_or_default();
    let timeline = store.timeline_day(day).unwrap_or_default();

    let (doc, page1, layer1) = PdfDocument::new(
        format!("Ftracker {day}"),
        Mm(210.0),
        Mm(297.0),
        "Layer 1",
    );
    let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
    let font_b = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;
    let layer = doc.get_page(page1).get_layer(layer1);

    let mut y = 280.0;
    let write = |layer: &PdfLayerReference, font: &IndirectFontRef, size: f64, y: f64, s: &str| {
        layer.use_text(s, size, Mm(12.0), Mm(y), font);
    };

    write(
        &layer,
        &font_b,
        16.0,
        y,
        &format!("Ftracker daily report  {day}  v{VERSION}"),
    );
    y -= 8.0;
    write(
        &layer,
        &font,
        11.0,
        y,
        &format!(
            "employee={}  pc={}  user={}",
            cfg.employee_name,
            cfg.machine_id,
            crate::config::windows_user()
        ),
    );
    y -= 6.0;
    write(
        &layer,
        &font,
        11.0,
        y,
        &format!(
            "active={} of {} required   idle excluded={}",
            format_hm(counted),
            format_hm(required),
            format_hm(idle)
        ),
    );
    y -= 6.0;
    write(
        &layer,
        &font,
        10.0,
        y,
        &format!(
            "loc_first={}  loc_last={}  fixes={fixes}",
            fmt_loc_opt(loc_first),
            fmt_loc_opt(loc_last)
        ),
    );
    y -= 8.0;
    write(&layer, &font_b, 12.0, y, "Category hours");
    y -= 6.0;
    for name in ["Browser", "IDE", "Office", "Other"] {
        let s = cats
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| *v)
            .unwrap_or(0);
        write(&layer, &font, 10.0, y, &format!("{name}: {}", format_hm(s)));
        y -= 5.0;
    }

    y -= 4.0;
    write(
        &layer,
        &font_b,
        12.0,
        y,
        &format!("Screenshot summary ({} unique)", shots.len()),
    );
    y -= 6.0;

    // Contact-sheet thumbs then full shots on following pages.
    let mut current_layer = layer;
    let mut pages: Vec<(PdfPageIndex, PdfLayerIndex)> = vec![(page1, layer1)];

    let thumb_w = 40.0;
    let mut x = 12.0;
    let thumb_row_y = y;
    let mut col = 0;
    for (i, (ts, process, jpeg)) in shots.iter().enumerate() {
        if col == 4 {
            col = 0;
            x = 12.0;
            y -= 32.0;
            if y < 40.0 {
                let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "Layer 1");
                pages.push((p, l));
                current_layer = doc.get_page(p).get_layer(l);
                y = 280.0;
            }
        }
        if let Ok(img) = ::image::load_from_memory(jpeg) {
            let thumb = ::image::DynamicImage::from(img.thumbnail(320, 200));
            let pdf_img = Image::from_dynamic_image(&thumb);
            let transform = ImageTransform {
                translate_x: Some(Mm(x)),
                translate_y: Some(Mm(y - 28.0)),
                dpi: Some(72.0),
                ..Default::default()
            };
            pdf_img.add_to_layer(current_layer.clone(), transform);
            let t = Local
                .timestamp_opt(*ts, 0)
                .single()
                .map(|t| t.format("%H:%M").to_string())
                .unwrap_or_default();
            current_layer.use_text(
                format!("{t} {process}"),
                7.0,
                Mm(x),
                Mm(y - 30.0),
                &font,
            );
            let _ = (i, thumb_w, thumb_row_y);
        }
        x += 48.0;
        col += 1;
    }

    y -= 40.0;
    if y < 40.0 || !shots.is_empty() {
        let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "Layer 1");
        current_layer = doc.get_page(p).get_layer(l);
        y = 280.0;
        current_layer.use_text("New screenshots", 12.0, Mm(12.0), Mm(y), &font_b);
        y -= 10.0;
    }

    for (ts, process, jpeg) in &shots {
        if y < 90.0 {
            let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "Layer 1");
            current_layer = doc.get_page(p).get_layer(l);
            y = 280.0;
        }
        let t = Local
            .timestamp_opt(*ts, 0)
            .single()
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_default();
        current_layer.use_text(
            format!("{t}  {process}"),
            9.0,
            Mm(12.0),
            Mm(y),
            &font,
        );
        y -= 5.0;
        if let Ok(img) = ::image::load_from_memory(jpeg) {
            let (w, h) = ::image::GenericImageView::dimensions(&img);
            let scaled = if w > 1200 {
                img.resize(
                    1200,
                    1200 * h / w.max(1),
                    ::image::imageops::FilterType::Triangle,
                )
            } else {
                img
            };
            let pdf_img = Image::from_dynamic_image(&scaled);
            pdf_img.add_to_layer(
                current_layer.clone(),
                ImageTransform {
                    translate_x: Some(Mm(12.0)),
                    translate_y: Some(Mm((y - 70.0).max(20.0))),
                    dpi: Some(110.0),
                    ..Default::default()
                },
            );
            y -= 75.0;
        }
    }

    if y < 80.0 {
        let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "Layer 1");
        current_layer = doc.get_page(p).get_layer(l);
        y = 280.0;
    }
    current_layer.use_text("Active timeline", 12.0, Mm(12.0), Mm(y), &font_b);
    y -= 6.0;
    for (ts, kind, process, title, category, lat, lon) in timeline.iter().take(80) {
        if y < 20.0 {
            let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "Layer 1");
            current_layer = doc.get_page(p).get_layer(l);
            y = 280.0;
        }
        let t = Local
            .timestamp_opt(*ts, 0)
            .single()
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_default();
        let loc = match (lat, lon) {
            (Some(a), Some(b)) => format!("{a:.3},{b:.3}"),
            _ => "unavailable".into(),
        };
        let line = format!("{t} {kind} {category} {process} {title} loc={loc}");
        let clipped: String = line.chars().take(110).collect();
        current_layer.use_text(clipped, 8.0, Mm(12.0), Mm(y), &font);
        y -= 4.0;
    }

    let mut buf = BufWriter::new(Vec::new());
    doc.save(&mut buf)?;
    Ok(buf.into_inner()?)
}

/// Landscape A4: last 30 days, one row per date, grid table.
pub fn build_month_pdf(
    cfg: &AppConfig,
    windows_user: &str,
    from: &str,
    to: &str,
    rows: &[MonthDayRow],
) -> Result<Vec<u8>> {
    use printpdf::{Color, Rgb};

    let (doc, page1, layer1) = PdfDocument::new(
        format!("Ftracker month {from} {to}"),
        Mm(297.0),
        Mm(210.0),
        "Layer 1",
    );
    let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
    let font_b = doc.add_builtin_font(BuiltinFont::HelveticaBold)?;

    let tracked: i64 = rows.iter().map(|r| r.counted_s).sum();
    let required: i64 = rows.iter().map(|r| r.required_s).sum();
    let idle: i64 = rows.iter().map(|r| r.idle_s).sum();
    let short_n = rows.iter().filter(|r| r.short).count();
    let ok_n = rows.iter().filter(|r| !r.weekend && !r.short).count();
    let br: i64 = rows.iter().map(|r| r.browser_s).sum();
    let ide: i64 = rows.iter().map(|r| r.ide_s).sum();
    let off: i64 = rows.iter().map(|r| r.office_s).sum();
    let oth: i64 = rows.iter().map(|r| r.other_s).sum();

    let mut layer = doc.get_page(page1).get_layer(layer1);
    let mut y = 198.0;
    layer.use_text(
        format!("Ftracker  —  last 30 days daily hours  v{VERSION}"),
        14.0,
        Mm(12.0),
        Mm(y),
        &font_b,
    );
    y -= 7.0;
    layer.use_text(
        format!(
            "Employee: {}    Windows user: {}    PC: {}",
            cfg.employee_name, windows_user, cfg.machine_id
        ),
        10.0,
        Mm(12.0),
        Mm(y),
        &font,
    );
    y -= 5.5;
    layer.use_text(
        format!(
            "Period: {from}  to  {to}    Generated: {}",
            Local::now().format("%Y-%m-%d %H:%M")
        ),
        9.0,
        Mm(12.0),
        Mm(y),
        &font,
    );
    y -= 5.5;
    layer.use_text(
        format!(
            "Tracked {} of {} required    Idle excluded {}    Weekdays OK {}    Short {}    Browser {}  IDE {}  Office {}  Other {}",
            format_hm(tracked),
            format_hm(required),
            format_hm(idle),
            ok_n,
            short_n,
            format_hm(br),
            format_hm(ide),
            format_hm(off),
            format_hm(oth)
        ),
        8.0,
        Mm(12.0),
        Mm(y),
        &font,
    );
    y -= 8.0;

    let headers = [
        "Date", "Day", "Tracked", "Required", "Idle", "Status", "Browser", "IDE", "Office",
        "Other", "Shots", "Location first / last",
    ];
    let widths: [f64; 12] = [24.0, 14.0, 20.0, 20.0, 16.0, 20.0, 18.0, 16.0, 18.0, 16.0, 14.0, 57.0];
    let left = 12.0;
    let row_h = 6.2;
    let table_w: f64 = widths.iter().sum();

    let stroke = Color::Rgb(Rgb::new(0.12, 0.22, 0.45, None));
    let header_fill = Color::Rgb(Rgb::new(0.85, 0.91, 1.0, None));
    let weekend_fill = Color::Rgb(Rgb::new(0.95, 0.95, 0.96, None));
    let short_fill = Color::Rgb(Rgb::new(1.0, 0.92, 0.92, None));
    let white = Color::Rgb(Rgb::new(1.0, 1.0, 1.0, None));

    let mut draw_header = true;
    for (i, row) in rows.iter().enumerate() {
        if y < 18.0 + row_h {
            let (p, l) = doc.add_page(Mm(297.0), Mm(210.0), "Layer 1");
            layer = doc.get_page(p).get_layer(l);
            y = 198.0;
            layer.use_text(
                format!(
                    "Ftracker daily hours  {}  {}  {}–{}  (cont.)",
                    cfg.employee_name, cfg.machine_id, from, to
                ),
                10.0,
                Mm(12.0),
                Mm(y),
                &font_b,
            );
            y -= 8.0;
            draw_header = true;
        }
        if draw_header {
            paint_row_bg(&layer, left, y, table_w, row_h, header_fill.clone());
            stroke_rect(&layer, left, y, table_w, row_h, stroke.clone());
            draw_col_lines(&layer, left, y, row_h, &widths, stroke.clone());
            let mut x = left;
            for (h, w) in headers.iter().zip(widths.iter()) {
                layer.use_text(*h, 7.0, Mm(x + 1.2), Mm(y + 1.8), &font_b);
                x += *w;
            }
            y -= row_h;
            draw_header = false;
        }
        let fill = if row.short {
            short_fill.clone()
        } else if row.weekend {
            weekend_fill.clone()
        } else {
            white.clone()
        };
        paint_row_bg(&layer, left, y, table_w, row_h, fill);
        stroke_rect(&layer, left, y, table_w, row_h, stroke.clone());
        draw_col_lines(&layer, left, y, row_h, &widths, stroke.clone());

        let status = if row.weekend {
            "weekend"
        } else if row.short {
            "SHORT"
        } else {
            "OK"
        };
        let loc = {
            let s = format!("{} / {}", row.loc_first, row.loc_last);
            s.chars().take(42).collect::<String>()
        };
        let cells = [
            row.day.clone(),
            row.weekday.clone(),
            format_hm(row.counted_s),
            format_hm(row.required_s),
            format_hm(row.idle_s),
            status.into(),
            format_hm(row.browser_s),
            format_hm(row.ide_s),
            format_hm(row.office_s),
            format_hm(row.other_s),
            row.shots.to_string(),
            loc,
        ];
        let mut x = left;
        for (cell, w) in cells.iter().zip(widths.iter()) {
            layer.use_text(cell, 6.5, Mm(x + 1.2), Mm(y + 1.7), &font);
            x += *w;
        }
        y -= row_h;
        let _ = i;
    }

    let mut buf = BufWriter::new(Vec::new());
    doc.save(&mut buf)?;
    Ok(buf.into_inner()?)
}

#[cfg(test)]
mod month_pdf_tests {
    use super::*;
    use crate::report::day_totals::MonthDayRow;

    #[test]
    fn month_pdf_is_pdf_with_table_days() {
        let cfg = crate::config::AppConfig {
            employee_name: "Alice".into(),
            machine_id: "PC1".into(),
            ..Default::default()
        };
        let row = |day: &str, counted: i64, weekend: bool| MonthDayRow {
            day: day.into(),
            weekday: if weekend { "Sat".into() } else { "Mon".into() },
            counted_s: counted,
            idle_s: 600,
            required_s: if weekend { 0 } else { 8 * 3600 },
            weekend,
            short: !weekend && counted < 8 * 3600,
            browser_s: 3600,
            ide_s: counted.saturating_sub(3600),
            office_s: 0,
            other_s: 0,
            loc_first: "unavailable".into(),
            loc_last: "unavailable".into(),
            loc_fixes: 0,
            shots: 2,
        };
        let rows = vec![
            row("2026-09-14", 7 * 3600, false),
            row("2026-09-15", 0, true),
        ];
        let bytes = build_month_pdf(&cfg, "alice", "2026-09-14", "2026-10-13", &rows).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 400);
    }
}

fn stroke_rect(
    layer: &printpdf::PdfLayerReference,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: printpdf::Color,
) {
    use printpdf::{Line, Point};
    layer.set_outline_color(color);
    layer.set_outline_thickness(0.25);
    layer.add_shape(Line {
        points: vec![
            (Point::new(Mm(x), Mm(y)), false),
            (Point::new(Mm(x + w), Mm(y)), false),
            (Point::new(Mm(x + w), Mm(y + h)), false),
            (Point::new(Mm(x), Mm(y + h)), false),
        ],
        is_closed: true,
        has_fill: false,
        has_stroke: true,
        is_clipping_path: false,
    });
}

fn paint_row_bg(
    layer: &printpdf::PdfLayerReference,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: printpdf::Color,
) {
    use printpdf::{Line, Point};
    layer.set_fill_color(fill);
    layer.add_shape(Line {
        points: vec![
            (Point::new(Mm(x), Mm(y)), false),
            (Point::new(Mm(x + w), Mm(y)), false),
            (Point::new(Mm(x + w), Mm(y + h)), false),
            (Point::new(Mm(x), Mm(y + h)), false),
        ],
        is_closed: true,
        has_fill: true,
        has_stroke: false,
        is_clipping_path: false,
    });
}

fn draw_col_lines(
    layer: &printpdf::PdfLayerReference,
    left: f64,
    y: f64,
    h: f64,
    widths: &[f64],
    color: printpdf::Color,
) {
    use printpdf::{Line, Point};
    layer.set_outline_color(color);
    layer.set_outline_thickness(0.2);
    let mut x = left;
    for w in widths.iter().take(widths.len().saturating_sub(1)) {
        x += *w;
        layer.add_shape(Line {
            points: vec![
                (Point::new(Mm(x), Mm(y)), false),
                (Point::new(Mm(x), Mm(y + h)), false),
            ],
            is_closed: false,
            has_fill: false,
            has_stroke: true,
            is_clipping_path: false,
        });
    }
}
