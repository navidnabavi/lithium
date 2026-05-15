extern crate time;
use std::sync::{Mutex,Arc};
use std::collections::{HashMap};
use std::u64;
use std::usize;
use std::thread::JoinHandle;
use std::thread;
use std::time::{Duration};
use std::collections::BTreeMap;
use std::sync::mpsc::{Sender};
use std::sync::mpsc::channel;
use std::fs::remove_file;

#[derive(Debug)]
struct TimeUrl{
    url : String,
    time: u64, //unix timestamp
    size: usize
}

type TimeUrlRef = Box<TimeUrl>;

fn now() -> u64 {
    time::precise_time_ns()
}

impl TimeUrl  {
    fn new(url: String) -> TimeUrlRef {
        let _ref =
            Box::new(
                TimeUrl
                {
                    url: url.to_owned(),
                    time : now(),
                    size : 0
                }
        );
        return _ref;
    }
}

#[derive(Debug)]
pub enum HitMiss {
    Hit,
    Miss,
    Downloading,
}

#[allow(dead_code)]
pub struct CacheController {
    urls: BTreeMap<u64, String>,
    url_map: HashMap<String, TimeUrlRef>,
    size : usize,
    mutex: Arc<Mutex<bool>>,
    size_limit: usize,
    soft_limit_ratio : f64,
    sweep_time: usize,
    max_delete_per_iteration: usize,
}

impl CacheController{
    pub fn new() -> CacheController{
        let ret = CacheController {
            urls: BTreeMap::new(),
            url_map : HashMap::new(),
            size: 0,
            mutex: Arc::new(Mutex::new(false)),
            size_limit: 100000,
            soft_limit_ratio : 0.85,
            sweep_time : 10,
            max_delete_per_iteration: 100,
        };

        return ret;
    }

    #[allow(dead_code)]
    pub fn access(&mut self,url: &str) -> HitMiss {

        let is_some = self.url_map.get(url).is_some();
        if is_some {
            let state = self.update(url);
            return state;

        } else {
            self.push(url);
        }
        HitMiss::Miss
    }

    fn push(&mut self, url: &str){
        let data = TimeUrl::new(url.to_owned());

        self.urls.insert(data.time, url.to_owned());
        self.url_map.insert(url.to_owned(),data);
    }

    fn update(&mut self, url: &str) -> HitMiss {
        let value = self.url_map.get_mut(url);
        if let Some(v) = value {
            match v.size {
                0 => return HitMiss::Downloading,
                _ => {
                    self.urls.remove(&v.time);
                    v.time = now();
                    self.urls.insert(v.time, v.url.to_owned());


                    return HitMiss::Hit
                }
            }
        }else {
            panic!("What the hell");
        }
    }

    pub fn remove(&mut self,url :&str) {
        self.url_map.remove(url);
    }

    pub fn download_done(&mut self,url: &str,size: usize){

        let url_data = self.url_map.get_mut(url);
        if let Some(v) = url_data {
            v.size = size;
            self.size += v.size;
        } else {
            println!("unexpected!");
        }
    }

    pub fn set_size_limit(&mut self){

    }
    pub fn set_time_limit(&mut self){

    }

    pub fn stats(&self) {
        println!("Index Size {}", self.url_map.len());
        println!("Files Size {}", self.size);

    }

    pub fn dump(&self){
        for i in self.url_map.iter(){
            println!("{:?}", i);
        }
    }

    fn sweep(&mut self, file_deleter : &Sender<String>) -> usize {
        let max_delete = self.max_delete_per_iteration;
        let mut i=0;
        while i < max_delete && self.soft_limit_passed() {
            self.sweep_once(file_deleter);
            i += 1;
        }
        self.sweep_time
    }

    fn get_oldest(&mut self)-> Option<(u64,String)>{
        let time_url =self.urls.iter().nth(0);
        if let Some(time_url_value) = time_url {
            let _time = *time_url_value.0;
            let _url = time_url_value.1.to_owned();
            return Some((_time,_url));
        }
        return None
    }

    fn sweep_once(&mut self,file_deleter: &Sender<String>){
        let time_url = self.get_oldest();
        if let Some((time,url)) = time_url {
            let size = self.url_map.get(&url).unwrap().size; //TODO: Check for error
            self.size -= size;
            file_deleter.send(url.to_owned());
            self.url_map.remove(&url);
            self.urls.remove(&time);
            println!("Attemp to remove {}", url);
        };


    }

    pub fn soft_limit_passed(&self) -> bool {
        self.size  > (self.size_limit as f64 * self.soft_limit_ratio) as usize
    }
    pub fn hard_limit_passed(&self) -> bool {
        self.size > self.size_limit
    }

}

pub struct Sweeper {
    sweeper_handle : JoinHandle<()>,
    file_deleter_handle : JoinHandle<()>,
}

impl Sweeper {
    pub fn new(cache_controller :Arc<Mutex<CacheController>>,base_dir: String) -> Sweeper {
        let (tx,_rx) = channel();

        let sweeper_handle = thread::spawn(move ||{
                println!("Sweeper thread has been started");
                let delete_channel = tx;
                loop {
                    let mut mutex = cache_controller.lock().unwrap();
                    println!("sweeping once");
                    let delay = mutex.sweep(&delete_channel);
                    drop(mutex);
                    thread::sleep(Duration::from_secs(delay as u64))
                }
            }
        );


        let file_deleter_handle = thread::spawn(move ||{
                println!("File deleter thread has been started");
                let rx = _rx;
                while let Ok(message) = rx.recv() {
                    let file_path = format!("{}{}",base_dir,message);
                    println!("deleting  file {:?}", file_path);

                    remove_file(file_path);
                }

            }
        );

        return Sweeper{
            sweeper_handle : sweeper_handle,
            file_deleter_handle : file_deleter_handle
        }
    }

    pub fn join(& self){
    }
}

#[test]
fn test_cache() {
    let mut cache_controller = CacheController::new();
    let test1 = (cache_controller.access("hello"),
    cache_controller.access("hello2"),
    cache_controller.access("hello"));

    match test1 {
        (HitMiss::Miss,HitMiss,HitMiss::Downloading) => {},
        t => {
            panic!(format!("Expected miss, miss, downloading but {:?} found",t))
        }
    }
    cache_controller.download_done("hello", 10);

    match cache_controller.access("hello") {
        HitMiss::Hit => {}
        T => panic!(format!("Should be {:?} but it's {:?}",HitMiss::Hit,T))
    }
    cache_controller.stats();
}
