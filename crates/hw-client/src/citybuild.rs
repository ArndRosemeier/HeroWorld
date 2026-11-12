//! The city the player plays in, and planning another one behind the title.
//!
//! The player picks a city (its seed and its size) in a window on the title
//! and presses OK: that is the moment the new plan starts, on a worker
//! thread. If they pick again before it is done, the running plan is
//! cancelled and the next one starts only once the worker has answered, so
//! two cities are never planned (or held) at once.
//!
//! Ordering is certain, not timed: only the main thread cancels, a job's
//! result is used only if its own flag was never set, and every other
//! result is thrown away (on a thread of its own, since a big city takes a
//! moment to free).

use hw_core::{Cancel, Cancelled, Seed};
use hw_sim::GeneratedTown;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread::JoinHandle;

/// Which city: the world seed and about how many people live in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct City {
    pub seed: u64,
    pub people: f64,
}

/// One step of the city-size choice.
#[derive(Clone, Copy, Debug)]
pub struct Size {
    pub name: &'static str,
    pub people: f64,
    /// A line on what it is like.
    pub blurb: &'static str,
    /// About how long it takes to plan (on a good PC), for the window.
    pub wait: &'static str,
}

/// The sizes on offer, smallest first.
pub const SIZES: [Size; 7] = [
    Size {
        name: "Hamlet",
        people: 300.0,
        blurb: "A handful of houses where the country roads meet.",
        wait: "a moment",
    },
    Size {
        name: "Village",
        people: 3_000.0,
        blurb: "A main street, a square, fields all round.",
        wait: "a moment",
    },
    Size {
        name: "Town",
        people: 15_000.0,
        blurb: "Old centre, new edges, a river to cross.",
        wait: "a second or two",
    },
    Size {
        name: "City",
        people: hw_sim::DEFAULT_POPULATION,
        blurb: "Downtown towers, quarters with their own faces, a villain's corner.",
        wait: "a few seconds",
    },
    Size {
        name: "Big city",
        people: 250_000.0,
        blurb: "Districts beyond districts; taxis earn their keep.",
        wait: "several seconds",
    },
    Size {
        name: "Metropolis",
        people: 1_000_000.0,
        blurb: "A million people. You will not see the end of it from the spire.",
        wait: "ten seconds or more",
    },
    Size {
        name: "Megalopolis",
        people: 2_000_000.0,
        blurb: "City to the horizon. The largest we have tested.",
        wait: "twenty seconds or more",
    },
];

/// The size step nearest to `people` (by ratio).
pub fn size_of(people: f64) -> &'static Size {
    let ratio = |s: &Size| (s.people.max(1.0) / people.max(1.0)).ln().abs();
    SIZES
        .iter()
        .min_by(|a, b| ratio(a).total_cmp(&ratio(b)))
        .expect("there are sizes")
}

/// The plan on the worker: what it is for, its flag, its answer.
struct Job {
    city: City,
    cancel: Cancel,
    answer: Receiver<Result<GeneratedTown, Cancelled>>,
    thread: JoinHandle<()>,
}

/// At most one city being planned, and the one wanted after it.
#[derive(Default)]
pub struct CityBuilds {
    running: Option<Job>,
    next: Option<City>,
}

impl CityBuilds {
    /// Wants `city` planned, in place of anything planned or queued before.
    /// `live` is the city already in use: wanting it again only stops the
    /// others.
    pub fn want(&mut self, city: City, live: City) {
        // Still planning exactly this, and not told to stop: keep it.
        if let Some(job) = &self.running {
            if job.city == city && !job.cancel.is_cancelled() {
                self.next = None;
                return;
            }
            job.cancel.cancel();
        }
        self.next = (city != live).then_some(city);
        if self.running.is_none() {
            self.start_next();
        }
    }

    /// Stops everything: nothing is wanted any more (the title closed).
    pub fn stop(&mut self) {
        self.next = None;
        if let Some(job) = &self.running {
            job.cancel.cancel();
        }
    }

    /// True while a plan runs or waits to start.
    pub fn busy(&self) -> bool {
        self.running.is_some() || self.next.is_some()
    }

    /// The city that will come out of [`CityBuilds::poll`], if any.
    pub fn coming(&self) -> Option<City> {
        self.next.or_else(|| {
            self.running
                .as_ref()
                .filter(|j| !j.cancel.is_cancelled())
                .map(|j| j.city)
        })
    }

    /// The wanted city, once it is planned. Call it every frame.
    pub fn poll(&mut self) -> Option<(City, GeneratedTown)> {
        let job = self.running.as_ref()?;
        let answer = match job.answer.try_recv() {
            Ok(a) => a,
            Err(TryRecvError::Empty) => return None,
            // The worker died without answering: it panicked.
            Err(TryRecvError::Disconnected) => Err(Cancelled),
        };
        let job = self.running.take().expect("checked above");
        if let Err(panic) = job.thread.join() {
            std::panic::resume_unwind(panic);
        }
        let wanted = !job.cancel.is_cancelled();
        let out = match answer {
            Ok(town) if wanted => {
                // Wanting another city cancels this one first.
                assert!(self.next.is_none(), "a wanted plan with another queued");
                Some((job.city, town))
            }
            Ok(stale) => {
                dispose(stale);
                None
            }
            Err(Cancelled) => {
                assert!(!wanted, "a plan gave up without being cancelled");
                None
            }
        };
        if out.is_none() {
            self.start_next();
        }
        out
    }

    fn start_next(&mut self) {
        assert!(self.running.is_none(), "one plan at a time");
        let Some(city) = self.next.take() else {
            return;
        };
        let cancel = Cancel::new();
        let (tx, answer) = channel();
        let c = cancel.clone();
        let thread = std::thread::Builder::new()
            .name("city plan".into())
            .spawn(move || {
                engine::parallel::background_thread();
                let r = GeneratedTown::generate_or_stop(Seed(city.seed), city.people, &c);
                // Cancelled while finishing: free it here, not on the game's thread.
                let r = match r {
                    Ok(town) if c.is_cancelled() => {
                        drop(town);
                        Err(Cancelled)
                    }
                    r => r,
                };
                let _ = tx.send(r);
            })
            .expect("a thread for the city plan");
        self.running = Some(Job {
            city,
            cancel,
            answer,
            thread,
        });
    }
}

impl Drop for CityBuilds {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Frees a town on a thread of its own: a big city takes a while to drop.
pub fn dispose<T: Send + 'static>(town: T) {
    let _ = std::thread::Builder::new()
        .name("city free".into())
        .spawn(move || {
            engine::parallel::background_thread();
            drop(town)
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(b: &mut CityBuilds) -> Option<(City, GeneratedTown)> {
        let t = std::time::Instant::now();
        while b.busy() {
            if let Some(r) = b.poll() {
                return Some(r);
            }
            assert!(t.elapsed().as_secs() < 120, "the plan never finished");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        None
    }

    #[test]
    fn sizes_round_trip() {
        for s in &SIZES {
            assert_eq!(size_of(s.people).name, s.name);
        }
        assert_eq!(size_of(1e9).name, "Megalopolis");
        assert_eq!(size_of(0.0).name, "Hamlet");
        assert_eq!(size_of(hw_sim::DEFAULT_POPULATION).name, "City");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn only_the_last_choice_comes_out() {
        let live = City {
            seed: 1,
            people: 300.0,
        };
        let mut b = CityBuilds::default();
        // Changed twice before the first was done: the first is thrown away.
        b.want(
            City {
                seed: 2,
                people: 300.0,
            },
            live,
        );
        let last = City {
            seed: 3,
            people: 300.0,
        };
        b.want(last, live);
        assert_eq!(b.coming(), Some(last));
        let (city, town) = wait(&mut b).expect("the last city");
        assert_eq!(city, last);
        assert_eq!(town.population(), 300.0);
        assert!(!b.busy());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn going_back_to_the_live_city_stops_the_plan() {
        let live = City {
            seed: 1,
            people: 300.0,
        };
        let mut b = CityBuilds::default();
        b.want(
            City {
                seed: 5,
                people: 300.0,
            },
            live,
        );
        b.want(live, live);
        assert_eq!(b.coming(), None);
        assert!(wait(&mut b).is_none());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn wanting_the_same_city_twice_plans_it_once() {
        let live = City {
            seed: 1,
            people: 300.0,
        };
        let c = City {
            seed: 9,
            people: 300.0,
        };
        let mut b = CityBuilds::default();
        b.want(c, live);
        b.want(c, live);
        assert!(b.next.is_none());
        assert_eq!(wait(&mut b).map(|r| r.0), Some(c));
    }
}
