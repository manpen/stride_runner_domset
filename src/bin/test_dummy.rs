use std::hint::black_box;

use structopt::StructOpt;

#[derive(StructOpt)]
enum Mode {
    Normal,
    Sleep { milliseconds: u64 },
    SigTerm,
    NeverTerminate,
    Alloc { megabytes: usize },
}

fn main() {
    let opts = Mode::from_args();

    let term = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, std::sync::Arc::clone(&term))
        .unwrap();

    match opts {
        Mode::Normal => {}
        Mode::Sleep { milliseconds } => {
            std::thread::sleep(std::time::Duration::from_millis(milliseconds));
        }
        Mode::SigTerm => {
            while !term.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        Mode::NeverTerminate => loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
        },
        Mode::Alloc { megabytes } => {
            let mut vec = vec![0u8; megabytes * 1024 * 1024];
            for (i, x) in vec.iter_mut().enumerate() {
                *x = (i % 256) as u8;
            }
            black_box(vec);
        }
    }
}
