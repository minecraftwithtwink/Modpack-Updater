use anyhow::Result;
use lazy_static::lazy_static;
use ratatui::style::{Color, Modifier, Style};
use rodio::{Decoder, OutputStream, Sink, Source};
use std::io::Cursor;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct Song {
    title: &'static str,
    artist: &'static str,
    style: Style,
    data: &'static [u8],
}

const SECRET_MUSIC_DATA: &[u8] = include_bytes!("../assets/3095990638.ogg");
const SFX_DATA: &[u8] = include_bytes!("../assets/ryan-gosling.ogg");
const SCROLL_SFX_DATA: &[u8] = include_bytes!("../assets/scroll.ogg");
const CONFIRM_SFX_DATA: &[u8] = include_bytes!("../assets/confirm.ogg");
const CANCEL_SFX_DATA: &[u8] = include_bytes!("../assets/cancel.ogg");

lazy_static! {
    static ref SONG_LIST: Vec<Song> = vec![
        Song {
            title: " What Lies Beyond the Door ",
            artist: "from Enchantment of the Ring by Secret Stairways",
            style: Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
            data: include_bytes!("../assets/865456212.ogg"),
        },
        Song {
            title: " Onward, to Hy Breasail ",
            artist: "from Enchantment of the Ring by Secret Stairways",
            style: Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
            data: include_bytes!("../assets/1190812374.ogg"),
        },
        Song {
            title: "The Red Eye of Sauron",
            artist: "Grimdor",
            style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            data: include_bytes!("../assets/1091848676.ogg"),
        },
    ];

    static ref SECRET_SONG: Song = Song {
        title: "Nightcall",
        artist: "Kavinsky",
        style: Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        data: SECRET_MUSIC_DATA,
    };
}

enum MusicCommand { Play, TogglePause, Stop, Exit, PlaySecretTrack, PlaySfx, PlayScrollSfx, PlayConfirmSfx, PlayCancelSfx }

pub struct MusicPlayer {
    command_tx: Sender<MusicCommand>,
    pub is_paused: bool,
    current_song_index: Arc<Mutex<usize>>,
    secret_mode_active: Arc<Mutex<bool>>,
}

impl MusicPlayer {
    pub fn new() -> Result<Self> {
        let (command_tx, command_rx) = mpsc::channel();
        
        let current_song_index = Arc::new(Mutex::new(0));
        let secret_mode_active = Arc::new(Mutex::new(false));
        
        let current_song_index_clone = Arc::clone(&current_song_index);
        let secret_mode_active_clone = Arc::clone(&secret_mode_active);

        thread::spawn(move || {
            if let Ok((_stream, stream_handle)) = OutputStream::try_default() {
                if let Ok(sink) = Sink::try_new(&stream_handle) {
                    sink.set_volume(0.1);
                    let mut is_playing = false;

                    loop {
                        if let Ok(command) = command_rx.try_recv() {
                            match command {
                                MusicCommand::Play => {
                                    *secret_mode_active_clone.lock().unwrap() = false;
                                    is_playing = true; sink.play();
                                }
                                MusicCommand::PlaySecretTrack => {
                                    *secret_mode_active_clone.lock().unwrap() = true;
                                    is_playing = true;
                                    sink.clear();
                                    if let Ok(source) = Decoder::new(Cursor::new(SECRET_SONG.data)) {
                                        sink.append(source.repeat_infinite());
                                    }
                                    sink.play();
                                }
                                MusicCommand::PlaySfx => {
                                    if let Ok(source) = Decoder::new(Cursor::new(SFX_DATA)) {
                                        stream_handle.play_raw(source.convert_samples()).ok();
                                    }
                                }
                                MusicCommand::PlayScrollSfx => {
                                    if let Ok(source) = Decoder::new(Cursor::new(SCROLL_SFX_DATA)) {
                                        stream_handle.play_raw(source.convert_samples()).ok();
                                    }
                                }
                                MusicCommand::PlayConfirmSfx => {
                                    if let Ok(source) = Decoder::new(Cursor::new(CONFIRM_SFX_DATA)) {
                                        stream_handle.play_raw(source.convert_samples()).ok();
                                    }
                                }
                                // --- ADDED: Handler for the cancel sound ---
                                MusicCommand::PlayCancelSfx => {
                                    if let Ok(source) = Decoder::new(Cursor::new(CANCEL_SFX_DATA)) {
                                        stream_handle.play_raw(source.convert_samples()).ok();
                                    }
                                }
                                MusicCommand::TogglePause => {
                                    if sink.is_paused() { sink.play(); is_playing = true; }
                                    else { sink.pause(); is_playing = false; }
                                }
                                MusicCommand::Stop => { is_playing = false; sink.stop(); }
                                MusicCommand::Exit => break,
                            }
                        }

                        if is_playing && sink.empty() && !*secret_mode_active_clone.lock().unwrap() {
                            let mut index_guard = current_song_index_clone.lock().unwrap();
                            let song = &SONG_LIST[*index_guard];
                            if let Ok(source) = Decoder::new(Cursor::new(song.data)) {
                                sink.append(source);
                            }
                            *index_guard = (*index_guard + 1) % SONG_LIST.len();
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });

        Ok(Self { command_tx, is_paused: false, current_song_index, secret_mode_active })
    }

    pub fn get_current_song_info(&self) -> (String, String, Style) {
        if *self.secret_mode_active.lock().unwrap() {
            return (SECRET_SONG.title.to_string(), SECRET_SONG.artist.to_string(), SECRET_SONG.style);
        }
        let index_guard = self.current_song_index.lock().unwrap();
        let current_index = (*index_guard + SONG_LIST.len() - 1) % SONG_LIST.len();
        let song = &SONG_LIST[current_index];
        (song.title.to_string(), song.artist.to_string(), song.style)
    }

    pub fn play(&mut self) { self.is_paused = false; self.command_tx.send(MusicCommand::Play).ok(); }
    pub fn play_secret_track(&mut self) { self.is_paused = false; self.command_tx.send(MusicCommand::PlaySecretTrack).ok(); }
    pub fn play_sfx(&self) { self.command_tx.send(MusicCommand::PlaySfx).ok(); }
    pub fn play_scroll_sfx(&self) { self.command_tx.send(MusicCommand::PlayScrollSfx).ok(); }
    pub fn play_confirm_sfx(&self) { self.command_tx.send(MusicCommand::PlayConfirmSfx).ok(); }
    pub fn play_cancel_sfx(&self) { self.command_tx.send(MusicCommand::PlayCancelSfx).ok(); }
    pub fn toggle_pause(&mut self) { self.is_paused = !self.is_paused; self.command_tx.send(MusicCommand::TogglePause).ok(); }
    pub fn stop(&self) { self.command_tx.send(MusicCommand::Stop).ok(); self.command_tx.send(MusicCommand::Exit).ok(); }
}