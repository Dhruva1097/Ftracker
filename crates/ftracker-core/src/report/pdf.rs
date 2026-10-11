use anyhow::Result;
use chrono::{Local, TimeZone};
use printpdf::{
    BuiltinFont, Image, ImageTransform, IndirectFontRef, Mm, PdfDocument, PdfLayerIndex,
    PdfLayerReference, PdfPageIndex,
};
use std::io::BufWriter;

use crate::config::AppConfig;
use crate::domain::time::format_hm;
use crate::report::day_totals::{fmt_loc_opt, required_seconds};
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
