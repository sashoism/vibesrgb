use eframe::egui;
use openrgb::{data::Color, OpenRGB};
use std::{
    error::Error,
    sync::{atomic::AtomicUsize, Arc},
};

struct App {
    img_uri: Option<String>,
    leds: Vec<Option<egui::Vec2>>,
    selected_led: Arc<AtomicUsize>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui_extras::install_image_loaders(ctx);
        egui::SidePanel::right("LEDs")
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("LEDs");
                let selected_led = self.selected_led.load(std::sync::atomic::Ordering::Relaxed);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (led_id, marker) in self.leds.iter().enumerate() {
                        let label = ui.selectable_label(
                            led_id == selected_led,
                            if let Some(marker) = marker {
                                format!("{:2}: ({:.2}, {:.2})", led_id, marker.x, marker.y)
                            } else {
                                format!("{:2}: (unplaced)", led_id)
                            },
                        );

                        if label.gained_focus() {
                            label.scroll_to_me(Some(egui::Align::Center));
                        }

                        if label.clicked() {
                            self.selected_led
                                .store(led_id, std::sync::atomic::Ordering::Relaxed);
                        };
                    }
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button("Load image…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.img_uri = Some(format!("file://{}", path.display()));
                }
            }

            if let Some(img_uri) = &self.img_uri {
                let image = ui.image(img_uri).interact(egui::Sense::click());
                if image.clicked_by(egui::PointerButton::Primary) {
                    let pos = image.interact_pointer_pos().unwrap();
                    let loc = (pos - image.rect.min) / (image.rect.max - image.rect.min);
                    let selected_led = self.selected_led.load(std::sync::atomic::Ordering::Relaxed);
                    self.leds[selected_led] = Some(loc);
                }

                if ui.button("Save configuration").clicked() {
                    println!("aspect_ratio={:?} leds={:?}", image.rect.aspect_ratio(), self.leds);
                    if let Some(path) = rfd::FileDialog::new().save_file() {
                        let mut file = std::fs::File::create(path).unwrap();
                        let config = (image.rect.aspect_ratio(), &self.leds);
                        serde_json::to_writer(&mut file, &config).unwrap();
                    }
                }
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let client = OpenRGB::connect_to(("localhost", 6742)).await?;
    let controller_id = 0;
    let num_leds = client.get_controller(controller_id).await?.leds.len();

    let selected_led = Arc::new(AtomicUsize::new(0));

    let app = App {
        img_uri: None,
        leds: vec![None; num_leds],
        selected_led: selected_led.clone(),
    };

    let selected_led_for_thread = selected_led.clone();
    tokio::spawn(async move {
        let mut elapsed = 0;
        loop {
            let led_id = selected_led_for_thread.load(std::sync::atomic::Ordering::Relaxed);
            client
                .update_leds(controller_id, vec![Color::new(0, 0, 0); num_leds as usize])
                .await
                .unwrap();
            if elapsed == 250 {
                client
                    .update_led(controller_id, led_id as i32, Color::new(255, 0, 0))
                    .await
                    .unwrap();
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            elapsed += 250;
            elapsed %= 500;
        }
    });

    eframe::run_native(
        "VibesRGB",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Box::new(app)),
    )?;

    Ok(())
}
