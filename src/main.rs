use eframe::egui;
use midly::{MetaMessage, Smf, TrackEventKind};
use rfd::FileDialog;
use std::fs;

struct MidiNote {
    pitch: u8,
    start_time: f32,
    duration: f32,
}

struct TrackData {
    name: String,
    notes: Vec<MidiNote>,
}

struct MidiVisualizer {
    tracks: Option<Vec<TrackData>>,
    selected_track: usize,
    ref_note: i32,
    ref_spacing: f32, // Spacing in pixels for the reference note
    px_per_beat: f32, // How many pixels one musical beat occupies
    file_path: String,
    export_status: String,
    scroll_offset: f32, // Horizontal scroll position
}

impl Default for MidiVisualizer {
    fn default() -> Self {
        Self {
            tracks: None,
            selected_track: 0,
            ref_note: 60,       // C4
            ref_spacing: 10.0,  // Base spacing for C4
            px_per_beat: 200.0, // Length of one beat
            file_path: "No file loaded".to_string(),
            export_status: String::new(),
            scroll_offset: 0.0,
        }
    }
}

impl MidiVisualizer {
    fn load_midi(&mut self, path: std::path::PathBuf) {
        let Ok(data) = fs::read(&path) else { return };
        let Ok(smf) = Smf::parse(&data) else { return };

        let ticks_per_beat = match smf.header.timing {
            midly::Timing::Metrical(t) => t.as_int() as f32,
            _ => 480.0,
        };

        let mut parsed_tracks = Vec::new();
        for (i, track) in smf.tracks.into_iter().enumerate() {
            let mut notes = Vec::new();
            let mut current_ticks = 0u32;
            let mut active_notes = std::collections::HashMap::new();
            let mut track_name = format!("Track {}", i);

            for event in track {
                current_ticks += event.delta.as_int();
                match event.kind {
                    TrackEventKind::Meta(MetaMessage::TrackName(name)) => {
                        if let Ok(s) = std::str::from_utf8(name) {
                            track_name = s.to_string();
                        }
                    }
                    TrackEventKind::Midi { message, .. } => match message {
                        midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                            active_notes.insert(key.as_int(), current_ticks);
                        }
                        midly::MidiMessage::NoteOn { key, .. }
                        | midly::MidiMessage::NoteOff { key, .. } => {
                            if let Some(start) = active_notes.remove(&key.as_int()) {
                                notes.push(MidiNote {
                                    pitch: key.as_int(),
                                    start_time: start as f32 / ticks_per_beat,
                                    duration: (current_ticks - start) as f32 / ticks_per_beat,
                                });
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            if !notes.is_empty() {
                notes.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());
                parsed_tracks.push(TrackData {
                    name: track_name,
                    notes,
                });
            }
        }
        self.tracks = Some(parsed_tracks);
        self.file_path = path.to_string_lossy().into_owned();
        self.selected_track = 0;
        self.scroll_offset = 0.0;
    }

    fn calculate_spacing(&self, pitch: u8) -> f32 {
        // f = 440 * 2^((n-69)/12)
        let ref_freq = 440.0 * 2.0f32.powf((self.ref_note as f32 - 69.0) / 12.0);
        let note_freq = 440.0 * 2.0f32.powf((pitch as f32 - 69.0) / 12.0);
        // Spacing is wavelength: S = S_ref * (F_ref / F_note)
        self.ref_spacing * (ref_freq / note_freq)
    }

    fn generate_svg(&self) -> String {
        let mut svg_content = String::new();
        let mut max_x = 0.0;

        if let Some(tracks) = &self.tracks {
            if let Some(track_data) = tracks.get(self.selected_track) {
                for note in &track_data.notes {
                    let spacing = self.calculate_spacing(note.pitch);
                    let start_x = note.start_time * self.px_per_beat;
                    let duration_px = note.duration * self.px_per_beat;

                    // We draw bars starting at the note onset until the duration is exhausted
                    let mut offset = 0.0;
                    while offset < duration_px {
                        let current_x = start_x + offset;
                        svg_content.push_str(&format!(
                            r#"<line x1="{:.2}" y1="0" x2="{:.2}" y2="100" stroke="black" stroke-width="0.5" />"#,
                            current_x, current_x
                        ));
                        offset += spacing;
                        if offset > duration_px {
                            break;
                        }
                    }
                    let end_x = start_x + duration_px;
                    if end_x > max_x {
                        max_x = end_x;
                    }
                }
            }
        }

        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.2}" height="100">{}</svg>"#,
            max_x + 50.0,
            svg_content
        )
    }
}

impl eframe::App for MidiVisualizer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("sidebar").show(ctx, |ui| {
            ui.heading("Musical Comb Designer");

            if ui.button("📂 Load MIDI").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("midi", &["mid", "midi"])
                    .pick_file()
                {
                    self.load_midi(path);
                }
            }

            ui.label(format!("File: {}", self.file_path));
            ui.separator();

            if let Some(tracks) = &self.tracks {
                ui.label("Select Track:");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for i in 0..tracks.len() {
                            let track = &tracks[i];
                            if ui
                                .selectable_label(
                                    self.selected_track == i,
                                    format!("{}: {} ({} notes)", i, track.name, track.notes.len()),
                                )
                                .clicked()
                            {
                                self.selected_track = i;
                            }
                        }
                    });
            }

            ui.separator();
            ui.label("Physics Calibration");
            ui.add(egui::Slider::new(&mut self.ref_note, 0..=127).text("Ref Note (MIDI)"));
            ui.add(egui::Slider::new(&mut self.ref_spacing, 0.5..=50.0).text("Ref Spacing (px)"));
            ui.add(egui::Slider::new(&mut self.px_per_beat, 10.0..=2000.0).text("Pixels per Beat"));

            ui.separator();
            ui.label("Timeline View");
            if ui.button("⏮ Jump to Start of Notes").clicked() {
                if let Some(tracks) = &self.tracks {
                    if let Some(track) = tracks.get(self.selected_track) {
                        if let Some(first_note) = track.notes.first() {
                            self.scroll_offset = first_note.start_time * self.px_per_beat - 50.0;
                        }
                    }
                }
            }
            ui.add(
                egui::DragValue::new(&mut self.scroll_offset)
                    .prefix("Scroll X: ")
                    .speed(5.0),
            );

            ui.separator();
            if ui.button("🖼 Export SVG").clicked() {
                if let Some(path) = FileDialog::new()
                    .set_file_name("comb_pattern.svg")
                    .save_file()
                {
                    let content = self.generate_svg();
                    let _ = fs::write(path, content);
                    self.export_status = "SVG Exported successfully.".to_string();
                }
            }

            ui.label(&self.export_status);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Determine total width needed for the timeline
            let mut total_width = ui.available_width();
            if let Some(tracks) = &self.tracks {
                if let Some(track_data) = tracks.get(self.selected_track) {
                    if let Some(last_note) = track_data.notes.last() {
                        let end_x = (last_note.start_time + last_note.duration) * self.px_per_beat;
                        total_width = total_width.max(end_x + 100.0);
                    }
                }
            }

            egui::ScrollArea::horizontal()
                .scroll_offset(egui::vec2(self.scroll_offset, 0.0))
                .show(ui, |ui| {
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(total_width, ui.available_height()),
                        egui::Sense::click(),
                    );
                    let rect = response.rect;

                    // Capture actual scroll offset from the ScrollArea to sync with the sidebar value
                    self.scroll_offset = (ui.clip_rect().left() - rect.left()).max(0.0);

                    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 25));

                    if let Some(tracks) = &self.tracks {
                        if let Some(track_data) = tracks.get(self.selected_track) {
                            for note in &track_data.notes {
                                let spacing = self.calculate_spacing(note.pitch);
                                let start_x = rect.min.x + (note.start_time * self.px_per_beat);
                                let duration_px = note.duration * self.px_per_beat;

                                let mut offset = 0.0;
                                while offset < duration_px {
                                    let current_x = start_x + offset;

                                    // Optimization: Only draw if within the visible clip rect
                                    if ui.clip_rect().x_range().contains(current_x) {
                                        painter.line_segment(
                                            [
                                                egui::pos2(current_x, rect.center().y - 60.0),
                                                egui::pos2(current_x, rect.center().y + 60.0),
                                            ],
                                            egui::Stroke::new(
                                                1.2,
                                                egui::Color32::from_rgb(0, 255, 200),
                                            ),
                                        );
                                    }
                                    offset += spacing;
                                    if spacing < 0.1 || offset > duration_px {
                                        break;
                                    }
                                }
                            }

                            // Draw a visual reference line
                            painter.line_segment(
                                [
                                    egui::pos2(rect.min.x, rect.center().y + 60.0),
                                    egui::pos2(rect.max.x, rect.center().y + 60.0),
                                ],
                                egui::Stroke::new(1.0, egui::Color32::GRAY),
                            );
                        }
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Please load a MIDI file to generate patterns.");
                        });
                    }
                });
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "MIDI Pattern Generator",
        native_options,
        Box::new(|_cc| Ok(Box::new(MidiVisualizer::default()))),
    )
}
