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
    scroll_to: Option<f32>,
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
            scroll_to: None,
        }
    }
}

struct CombSegment {
    start_time: f32,
    end_time: f32,
    spacing: f32,
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
        self.scroll_to = None;
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

        let segments = self.get_comb_segments();
        if segments.is_empty() {
            return format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="50" height="100"></svg>"#
            );
        }

        let x_offset = segments.first().unwrap().start_time * self.px_per_beat;
        let mut max_x: f32 = 0.0;

        for segment in &segments {
            let start_x = segment.start_time * self.px_per_beat;
            let end_x = segment.end_time * self.px_per_beat;
            let spacing = segment.spacing;

            if spacing > 0.1 {
                let first_tooth_index = (start_x / spacing).ceil() as i64;
                let mut current_x_abs = first_tooth_index as f32 * spacing;

                while current_x_abs < end_x {
                    let current_x_relative = current_x_abs - x_offset;
                    // Use a small epsilon to avoid floating point issues at the start
                    if current_x_relative >= -f32::EPSILON {
                        svg_content.push_str(&format!(
                            r#"<line x1="{:.2}" y1="0" x2="{:.2}" y2="100" stroke="black" stroke-width="0.5" />"#,
                            current_x_relative, current_x_relative
                        ));
                    }
                    current_x_abs += spacing;
                }
            }
            max_x = max_x.max(end_x);
        }

        let total_width = max_x - x_offset;

        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.2}" height="100">{}</svg>"#,
            total_width + 50.0,
            svg_content
        )
    }

    fn get_comb_segments(&self) -> Vec<CombSegment> {
        let Some(tracks) = &self.tracks else {
            return vec![];
        };
        let Some(track_data) = tracks.get(self.selected_track) else {
            return vec![];
        };

        if track_data.notes.is_empty() {
            return vec![];
        }

        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        enum EventType {
            On,
            Off,
        }
        struct Event {
            time: f32,
            kind: EventType,
            pitch: u8,
        }
        let mut events = Vec::new();
        for note in &track_data.notes {
            events.push(Event {
                time: note.start_time,
                kind: EventType::On,
                pitch: note.pitch,
            });
            events.push(Event {
                time: note.start_time + note.duration,
                kind: EventType::Off,
                pitch: note.pitch,
            });
        }
        events.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.kind.cmp(&b.kind))
        });

        let mut segments = Vec::new();
        let mut active_pitches = std::collections::BTreeSet::new();
        let mut last_time = if events.is_empty() {
            0.0
        } else {
            events[0].time
        };

        for event in &events {
            let current_time = event.time;
            if current_time > last_time && !active_pitches.is_empty() {
                if let Some(&highest_pitch) = active_pitches.iter().last() {
                    segments.push(CombSegment {
                        start_time: last_time,
                        end_time: current_time,
                        spacing: self.calculate_spacing(highest_pitch),
                    });
                }
            }

            match event.kind {
                EventType::On => {
                    active_pitches.insert(event.pitch);
                }
                EventType::Off => {
                    active_pitches.remove(&event.pitch);
                }
            }
            last_time = current_time;
        }

        // Merge segments
        if segments.is_empty() {
            return vec![];
        }

        let mut merged = Vec::new();
        let mut iter = segments.into_iter();
        let mut current = iter.next().unwrap();

        for next in iter {
            // Using an epsilon for f32 comparison
            if (next.spacing - current.spacing).abs() < f32::EPSILON
                && (next.start_time - current.end_time).abs() < f32::EPSILON
            {
                current.end_time = next.end_time;
            } else {
                merged.push(current);
                current = next;
            }
        }
        merged.push(current);

        merged
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
                            self.scroll_to = Some(first_note.start_time * self.px_per_beat - 50.0);
                        }
                    }
                }
            }
            let mut dv_offset = self.scroll_offset;
            if ui
                .add(
                    egui::DragValue::new(&mut dv_offset)
                        .prefix("Scroll X: ")
                        .speed(5.0),
                )
                .changed()
            {
                self.scroll_to = Some(dv_offset);
            }

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

            let mut scroll_area = egui::ScrollArea::horizontal();
            if let Some(offset) = self.scroll_to.take() {
                scroll_area = scroll_area.scroll_offset(egui::vec2(offset, 0.0));
                // Update display state immediately for responsiveness
                self.scroll_offset = offset;
            }

            scroll_area.show(ui, |ui| {
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
                        let segments = self.get_comb_segments();
                        for segment in &segments {
                            let start_x_abs = segment.start_time * self.px_per_beat;
                            let end_x_abs = segment.end_time * self.px_per_beat;
                            let spacing = segment.spacing;

                            if spacing > 0.1 {
                                let first_tooth_index = (start_x_abs / spacing).ceil() as i64;
                                let mut current_x_abs = first_tooth_index as f32 * spacing;

                                while current_x_abs < end_x_abs {
                                    let current_x_screen = rect.min.x + current_x_abs;
                                    if ui.clip_rect().x_range().contains(current_x_screen) {
                                        painter.line_segment(
                                            [
                                                egui::pos2(
                                                    current_x_screen,
                                                    rect.center().y - 60.0,
                                                ),
                                                egui::pos2(
                                                    current_x_screen,
                                                    rect.center().y + 60.0,
                                                ),
                                            ],
                                            egui::Stroke::new(
                                                1.2,
                                                egui::Color32::from_rgb(0, 255, 200),
                                            ),
                                        );
                                    }
                                    current_x_abs += spacing;
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
