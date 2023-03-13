// SPDX-FileCopyrightText: 2022  Emmanuele Bassi
// SPDX-License-Identifier: GPL-3.0-or-later

use core::cmp::Ordering;
use std::path::PathBuf;

use color_thief::{get_palette, ColorFormat};
use glib::clone;
use gtk::{gdk, gio, glib, prelude::*};
use log::{debug, warn};

use lofty::{Accessor, Tag, TagType};

use crate::{
    audio::{Queue, Song},
    config::APPLICATION_ID,
}

pub fn settings_manager() -> gio::Settings {
    // We ship a single schema for both default and development profiles
    let app_id = APPLICATION_ID.trim_end_matches(".Devel");
    gio::Settings::new(app_id)
}

pub fn format_remaining_time(t: i64) -> String {
    // We use an explicit LRM character so the MINUS SIGN character
    // stays in front of the remaining time even in RTL locales, instead
    // of getting flipped at the end of the text
    format!("\u{200e}\u{2212}{}:{:02}", (t - (t % 60)) / 60, t % 60)
}

pub fn format_time(t: i64) -> String {
    format!("{}:{:02}", (t - (t % 60)) / 60, t % 60)
}

// The base cover size is 192px, but we need to account for HiDPI;
// better to scale down when rendering on displays with a scaling
// factor of 1 than having to scale up on displays with a scaling
// factor of 2.
const COVER_SIZE: i32 = 256 * 2;

pub fn load_cover_texture(buffer: &glib::Bytes) -> Option<gdk_pixbuf::Pixbuf> {
    let stream = gio::MemoryInputStream::from_bytes(buffer);

    if let Ok(pixbuf) =
        gdk_pixbuf::Pixbuf::from_stream_at_scale(&stream, -1, -1, true, gio::Cancellable::NONE)
    {
        let width = pixbuf.width();
        let height = pixbuf.height();
        let ratio = width as f32 / height as f32;

        let w: i32;
        let h: i32;
        if ratio > 1.0 {
            w = COVER_SIZE.into();
            h = (COVER_SIZE as f32 / ratio) as i32;
        } else {
            w = (COVER_SIZE as f32 / ratio) as i32;
            h = COVER_SIZE.into();
        }

        pixbuf.scale_simple(w, h, gdk_pixbuf::InterpType::Bilinear)
    } else {
        warn!("Unable to load cover art");
        None
    }
}

pub fn cache_cover_art(uuid: &str, pixbuf: &gdk_pixbuf::Pixbuf) -> Option<PathBuf> {
    let mut cache_dir = glib::user_cache_dir();
    cache_dir.push("amberol");
    cache_dir.push("covers");
    glib::mkdir_with_parents(&cache_dir, 0o755);

    cache_dir.push(format!("{}.png", &uuid));
    let file = gio::File::for_path(&cache_dir);
    match file.create(gio::FileCreateFlags::NONE, gio::Cancellable::NONE) {
        Ok(stream) => {
            debug!("Creating cover data cache at {:?}", &cache_dir);
            pixbuf.save_to_streamv_async(
                &stream,
                "png",
                &[("tEXt::Software", "amberol")],
                gio::Cancellable::NONE,
                move |res| {
                    match res {
                        Err(e) => warn!("Unable to cache cover data: {}", e),
                        _ => (),
                    };
                },
            );
        }
        Err(e) => {
            if let Some(file_error) = e.kind::<glib::FileError>() {
                match file_error {
                    glib::FileError::Exist => (),
                    _ => {
                        warn!("Unable to create cache file: {}", e);
                        return None;
                    }
                };
            }
        }
    };

    Some(cache_dir)
}

fn color_format(has_alpha: bool) -> ColorFormat {
    if has_alpha {
        ColorFormat::Rgba
    } else {
        ColorFormat::Rgb
    }
}

pub fn load_palette(pixbuf: &gdk_pixbuf::Pixbuf) -> Option<Vec<gdk::RGBA>> {
    if let Ok(palette) = get_palette(
        pixbuf.pixel_bytes().unwrap().as_ref(),
        color_format(pixbuf.has_alpha()),
        5,
        4,
    ) {
        let colors: Vec<gdk::RGBA> = palette
            .iter()
            .map(|c| {
                gdk::RGBA::new(
                    c.r as f32 / 255.0,
                    c.g as f32 / 255.0,
                    c.b as f32 / 255.0,
                    1.0,
                )
            })
            .collect();

        return Some(colors);
    }

    None
}

struct HSLA {
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
    pub alpha: f32,
}

impl HSLA {
    fn max_channel(color: &gdk::RGBA) -> f32 {
        let max = if color.red() > color.green() {
            if color.red() > color.blue() {
                color.red()
            } else {
                color.blue()
            }
        } else {
            if color.green() > color.blue() {
                color.green()
            } else {
                color.blue()
            }
        };

        max
    }

    fn min_channel(color: &gdk::RGBA) -> f32 {
        let min = if color.red() > color.green() {
            if color.green() < color.blue() {
                color.green()
            } else {
                color.blue()
            }
        } else {
            if color.red() < color.blue() {
                color.red()
            } else {
                color.blue()
            }
        };

        min
    }

    fn from_rgba(color: &gdk::RGBA) -> Self {
        let max = HSLA::max_channel(color);
        let min = HSLA::min_channel(color);
        let lightness = (max + min) / 2.0;
        let mut saturation = 0.0;
        let mut hue = 0.0;
        if max != min {
            if lightness <= 0.5 {
                saturation = (max - min) / (max + min);
            } else {
                saturation = (max - min) / (2.0 - max - min);
            }

            let delta = max - min;
            if color.red() == max {
                hue = (color.green() - color.blue()) / delta;
            } else if color.green() == max {
                hue = 2.0 + (color.blue() - color.red()) / delta;
            } else if color.blue() == max {
                hue = 4.0 + (color.red() - color.green()) / delta;
            }

            hue *= 60.0;
            if hue < 0.0 {
                hue += 360.0;
            }
        }

        let alpha = color.alpha();

        Self {
            hue,
            lightness,
            saturation,
            alpha,
        }
    }

    fn to_rgba(&self) -> gdk::RGBA {
        if self.saturation == 0.0 {
            return gdk::RGBA::new(self.lightness, self.lightness, self.lightness, self.alpha);
        }

        let m2 = if self.lightness <= 0.5 {
            self.lightness * (1.0 + self.saturation)
        } else {
            self.lightness + self.saturation - self.lightness * self.saturation
        };
        let m1 = 2.0 * self.lightness - m2;

        let mut hue = self.hue + 120.0;
        while hue > 360.0 {
            hue -= 360.0;
        }
        while hue < 0.0 {
            hue += 360.0;
        }

        let red = if hue < 60.0 {
            m1 + (m2 - m1) * hue / 60.0
        } else if hue < 180.0 {
            m2
        } else if hue < 240.0 {
            m1 + (m2 - m1) * (240.0 - hue) / 60.0
        } else {
            m1
        };

        hue = self.hue;
        while hue > 360.0 {
            hue -= 360.0;
        }
        while hue < 0.0 {
            hue += 360.0;
        }

        let green = if hue < 60.0 {
            m1 + (m2 - m1) * hue / 60.0
        } else if hue < 180.0 {
            m2
        } else if hue < 240.0 {
            m1 + (m2 - m1) * (240.0 - hue) / 60.0
        } else {
            m1
        };

        hue = self.hue - 120.0;
        while hue > 360.0 {
            hue -= 360.0;
        }
        while hue < 0.0 {
            hue += 360.0;
        }

        let blue = if hue < 60.0 {
            m1 + (m2 - m1) * hue / 60.0
        } else if hue < 180.0 {
            m2
        } else if hue < 240.0 {
            m1 + (m2 - m1) * (240.0 - hue) / 60.0
        } else {
            m1
        };

        gdk::RGBA::new(red, green, blue, self.alpha)
    }

    fn complementary(&self) -> HSLA {
        let hue = if self.hue >= 180.0 {
            self.hue - 180.0
        } else {
            self.hue + 180.0
        };

        HSLA {
            hue,
            lightness: self.lightness,
            saturation: self.saturation,
            alpha: self.alpha,
        }
    }
}

pub fn complementary_color(color: &gdk::RGBA) -> gdk::RGBA {
    let hsla = HSLA::from_rgba(color);
    let complementary = hsla.complementary();
    complementary.to_rgba()
}

// Convert a CIEXYZ color into CIELAB
//
// Formulas and constants are taken from:
//   https://en.wikipedia.org/wiki/CIELAB_color_space#From_CIEXYZ_to_CIELAB
fn lab_from_xyz(xyz: [f32; 3]) -> [f32; 3] {
    let epsilon: f32 = 6.0 / 29.0;
    let kappa: f32 = 4.0 / 29.0;

    // We use the D65 standard illuminant constants, since we don't have any
    // other mean of getting a reference white
    let t_x = xyz[0] / 95.0489;
    let t_y = xyz[1] / 100.0;
    let t_z = xyz[2] / 108.8840;

    let epsilon_square = epsilon.powf(2.0);
    let epsilon_cube = epsilon.powf(3.0);

    let f_x = if t_x > epsilon_cube {
        t_x.powf(1.0 / 3.0)
    } else {
        kappa + t_x / (3.0 * epsilon_square)
    };

    let f_y = if t_y > epsilon_cube {
        t_y.powf(1.0 / 3.0)
    } else {
        kappa + t_y / (3.0 * epsilon_square)
    };

    let f_z = if t_z > epsilon_cube {
        t_z.powf(1.0 / 3.0)
    } else {
        kappa + t_z / (3.0 * epsilon_square)
    };

    [116.0 * f_y - 16.0, 500.0 * (f_x - f_y), 200.0 * (f_y - f_z)]
}

// Compute the CIE76 color difference between two RGBA colors (we assume in sRGB
// space, because that's generally what GTK does; until GTK gets colorspace
// management for high dynamic ranges, this is the best we can do).
//
// CIE76 isn't that accurate, but it's good enough for us, considering the color
// space and ranges
pub fn color_distance(color_a: &gdk::RGBA, color_b: &gdk::RGBA) -> f32 {
    // Turn sRGB normalized colors into XYZ
    let xyz_a = srgb::xyz_from_normalised([color_a.red(), color_a.green(), color_a.blue()]);
    let xyz_b = srgb::xyz_from_normalised([color_b.red(), color_b.green(), color_b.blue()]);

    // Convert XYZ in Lab
    let lab_a = lab_from_xyz(xyz_a);
    let lab_b = lab_from_xyz(xyz_b);

    // The CIE76 distance is just the Euclidean vector distance
    let delta_l = (lab_b[0] - lab_a[0]) * (lab_b[0] - lab_a[0]);
    let delta_a = (lab_b[1] - lab_a[1]) * (lab_b[1] - lab_a[1]);
    let delta_b = (lab_b[2] - lab_a[2]) * (lab_b[2] - lab_a[2]);
    f32::sqrt(delta_l + delta_a + delta_b)
}

fn load_files_from_folder_internal(
    base: &gio::File,
    folder: &gio::File,
    recursive: bool,
) -> Vec<gio::File> {
    let mut enumerator = folder
        .enumerate_children(
            "standard::name,standard::type",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            None::<&gio::Cancellable>,
        )
        .expect("Unable to enumerate");

    let mut files = Vec::new();
    while let Some(info) = enumerator.next().and_then(|s| s.ok()) {
        let child = enumerator.child(&info);
        if recursive && info.file_type() == gio::FileType::Directory {
            let mut res = load_files_from_folder_internal(&base, &child, recursive);
            files.append(&mut res);
        } else if info.file_type() == gio::FileType::Regular {
            files.push(child.clone());
        }
    }
    files.sort_by_cached_key(|a| get_file_sort_key(a));

    files
}

pub fn get_file_sort_key(file: &gio::File) -> String {
    //Final sort key will look like: artistalbumtrack

    let file_path = file.path().expect("Unable to find file");
    let tagged_file = match lofty::read_from_path(&file_path) {
        Ok(f) => f,
        Err(e) => {
            warn!("Unable to open file {:?}: {}", file_path, e);
            return String::new();
        }
    };
    let default_tag = Tag::new(TagType::ID3v2);
    let tag = tagged_file
        .primary_tag()
        .unwrap_or(tagged_file.first_tag().unwrap_or(&default_tag));
    let mut artist = tag.artist().unwrap_or("0").to_lowercase();

    //Some artist metadata separates multiple artists by semicolons or commas - quick and dirty way of dealing with that.
    if artist.find(';') == None {
        artist = artist.split(',').next().unwrap_or("0").to_string();
    } else {
        artist = artist.split(';').next().unwrap_or("0").to_string();
    }

    let album = tag.album().unwrap_or("0");
    let track = tag.track().unwrap_or(0).to_string();
    let mut sort_key = String::new();
    sort_key.push_str(&artist);
    sort_key.push_str(&album);
    sort_key.push_str(&track);

    sort_key
}

pub fn cmp_two_files(a: &gio::File, b: &gio::File) -> Ordering {
    let a_key = get_file_sort_key(a);
    let b_key = get_file_sort_key(b);
    a_key.cmp(&b_key)
}

pub fn load_files_from_folder(folder: &gio::File, recursive: bool) -> Vec<gio::File> {
    use std::time::Instant;

    let now = Instant::now();
    let res = load_files_from_folder_internal(folder, folder, recursive);
    debug!(
        "Folder enumeration: {} us (recursive: {}), total files: {}",
        now.elapsed().as_micros(),
        recursive,
        res.len()
    );

    res
}

async fn store_current_pls(queue: &Queue) {
    let pls = glib::KeyFile::new();
    pls.set_string("playlist", "X-GNOME-Title", "Amberol's current playlist");

    pls.set_int64("playlist", "NumberOfEntries", queue.n_songs() as i64);

    let model = queue.model();
    for i in 0..model.n_items() {
        let item = model.item(i).unwrap();
        let song = item.downcast_ref::<Song>().unwrap();
        let path = song.file().path().expect("Unknown file");
        let path_str = path.to_string_lossy();
        pls.set_value("playlist", &format!("File{i}"), &path_str);
    }

    let mut pls_cache = glib::user_cache_dir();
    pls_cache.push("amberol");
    pls_cache.push("playlists");
    glib::mkdir_with_parents(&pls_cache, 0o755);

    pls_cache.push("current.pls");
    match pls.save_to_file(&pls_cache) {
        Ok(_) => debug!("Current playlist updated to: {:?}", &pls_cache),
        Err(e) => debug!("Unable to save current playlist: {e}"),
    }
}

pub fn store_playlist(queue: &Queue) {
    let ctx = glib::MainContext::default();
    ctx.spawn_local(clone!(@weak queue => async move {
        store_current_pls(&queue).await
    }));
}

pub fn load_cached_songs() -> Option<Vec<gio::File>> {
    let mut pls_cache = glib::user_cache_dir();
    pls_cache.push("amberol");
    pls_cache.push("playlists");
    pls_cache.push("current.pls");

    let pls = glib::KeyFile::new();
    match pls.load_from_file(&pls_cache, glib::KeyFileFlags::NONE) {
        Err(e) => {
            debug!("Unable to load current playlist: {e}");
            return None;
        }
        Ok(_) => (),
    }

    let n_entries: usize = match pls.int64("playlist", "NumberOfEntries") {
        Ok(n) => n as usize,
        Err(_) => 0,
    };

    let mut res = Vec::with_capacity(n_entries);

    for i in 0..n_entries {
        match pls.value("playlist", &format!("File{i}")) {
            Ok(p) => res.push(gio::File::for_path(p)),
            Err(e) => debug!("Skipping File{i} from playlist: {e}"),
        }
    }

    Some(res)
}

pub fn has_cached_playlist() -> bool {
    let mut pls_cache = glib::user_cache_dir();
    pls_cache.push("amberol");
    pls_cache.push("playlists");
    pls_cache.push("current.pls");

    pls_cache.exists()
}
