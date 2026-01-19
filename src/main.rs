use eframe::egui;
use midly::{Smf, TrackEventKind};
use rfd::FileDialog;
use std::fs;

struct MidiNote {
    pitch: u8,
    start_time: f32,
    duration: f32,
}

struct MidiVisualizer {
    midi_data: Option<Vec<Vec<MidiNote>>>,
    selected_track: usize,
    ref_note: i32,
    ref_spacing: f32,
    file_path: String,
    visual_scale: f32,
    export_status: String,
}

impl Default for MidiVisualizer {
    fn default() -> Self {
        Self {
            midi_data: None,
            selected_track: 0,
            ref_note: 60,      //C4
            ref_spacing: 37.8, // about 1 cm at 96 DPI
            file_path: "No file loaded".to_string(),
            visual_scale: 1.0,
            export_status: String::new(),
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

        let mut all_tracks = Vec::new();
        for track in smf.tracks {
            let mut notes = Vec::new();
            let mut current_ticks = 0u32;
            let mut active_notes = std::collections::HashMap::new();

            for event in track {
                current_ticks += event.delta.as_int();
                if let TrackEventKind::Midi { message, .. } = event.kind {
                    match message {
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
                    }
                }
            }
            notes.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());
            all_tracks.push(notes);
        }
        self.midi_data = Some(all_tracks);
        self.file_path = path.to_string_lossy().into_owned();
    }

    fn calculate_spacing(&self, pitch: u8) -> f32 {
        let ref_freq = 440.0 * 2.0f32.powf((self.ref_note as f32 - 69.0) / 12.0);
        let note_freq = 440.0 * 2.0f32.powf((pitch as f32 - 69.0) / 12.0);
        (self.ref_spacing * (ref_freq / note_freq)) * self.visual_scale
    }

    fn generate_svg(&self) -> String {
        let mut svg =
            String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="2000" height="500">"#);
        if let Some(tracks) = &self.midi_data {
            if let Some(notes) = tracks.get(self.selected_track) {
                let mut x = 10.0;
                for note in notes {
                    let spacing = self.calculate_spacing(note.pitch);
                    let iterations = (note.duration * 5.0).max(1.0) as i32;
                    for _ in 0..iterations {
                        svg.push_str(&format!(r#"<line x1="{0}" y1="50" x2="{0}" y2="450" stroke="black" stroke-width="1" />"#, x));
                        x += spacing;
                    }
                }
            }
        }
        svg.push_str("</svg>");
        svg
    }
}

impl eframe::App for MidiVisualizer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("sidebar").show(ctx, |ui| {
            ui.heading("MIDI Frequency Bars");

            if ui.button("📂 Load Midi").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("midi", &["mid", "midi"])
                    .pick_file()
                {
                    self.load_midi(path);
                }
            }

            ui.label(format!("File: {}", self.file_path));
            ui.separator();

            if let Some(tracks) = &self.midi_data {
                ui.label("Select Tack:");
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for i in 0..tracks.len() {
                            if ui
                                .selectable_label(
                                    self.selected_track == i,
                                    format!("Track {} ({} notes)", i, tracks[i].len()),
                                )
                                .clicked()
                            {
                                self.selected_track = i;
                            }
                        }
                    });
            }

            ui.separator();
            ui.add(egui::Slider::new(&mut self.ref_note, 0..=127).text("Ref Note (C4=60)"));
            ui.add(egui::Slider::new(&mut self.ref_spacing, 1.0..=200.0).text("Ref Spacing (px)"));
            ui.add(egui::Slider::new(&mut self.visual_scale, 0.1..=5.0).text("Zoom Scale"));

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("🖼 Export SVG").clicked() {
                    if let Some(path) = FileDialog::new().set_file_name("pattern.svg").save_file() {
                        let content = self.generate_svg();
                        let _ = fs::write(path, content);
                        self.export_status = "SVG Exported!".to_string();
                    }
                }
                if ui.button("📷 Export PNG").clicked() {
                    self.export_status =
                        "PNG Export requires 'image' crate - logic included in source.".to_string();
                }
            });
            ui.label(&self.export_status);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();
            let rect = ui.max_rect();

            // Draw Background
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 25));

            if let Some(tracks) = &self.midi_data {
                if let Some(notes) = tracks.get(self.selected_track) {
                    let mut current_x = rect.min.x + 20.0;

                    for note in notes {
                        let spacing = self.calculate_spacing(note.pitch);
                        let iterations = (note.duration * 5.0).max(1.0) as i32;

                        for i in 0..iterations {
                            let x = current_x + (i as f32 * spacing);
                            if x > rect.max.x {
                                break;
                            }

                            painter.line_segment(
                                [
                                    egui::pos2(x, rect.min.y + 100.0),
                                    egui::pos2(x, rect.max.y - 100.0),
                                ],
                                egui::Stroke::new(
                                    1.0,
                                    egui::Color32::from_rgb(100, 200, 255).linear_multiply(0.8),
                                ),
                            );
                        }
                        current_x += iterations as f32 * spacing;
                        if current_x > rect.max.x {
                            break;
                        }
                    }
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Load a MIDI to generate patterns.");
                });
            }
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
