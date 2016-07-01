extern crate time;
use std::sync::{Mutex,Arc};
use std::collections::{LinkedList,HashMap};
use std::u64;
use std::usize;
use std::thread::JoinHandle;
use std::thread;
use std::time::{SystemTime,Duration};
use std::collections::BTreeMap;
use std::sync::mpsc::{Sender,Receiver};
use std::sync::mpsc::channel;
use std::fs::remove_file;

#[derive(Debug,RustcEncodable, RustcDecodable)]
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
        // Rc::new(
            Box::new(
                TimeUrl
                {
                    url: url.to_owned(),
                    time : now(),
                    size : 0
                }
            // )
        );
        return _ref;
    }
}

#[derive(Debug,RustcEncodable, RustcDecodable)]
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
    // time_limit: u64,
    soft_limit_ratio : f64,
    sweep_time: usize,
    max_delete_per_iteration: usize
}

impl CacheController{
    pub fn new() -> CacheController{
        let mut ret = CacheController {
            urls: BTreeMap::new(),
            url_map : HashMap::new(),
            size: 0,
            mutex: Arc::new(Mutex::new(false)),
            // time_limit : u64::MAX,
            size_limit: usize::MAX,
            soft_limit_ratio : 0.85,
            sweep_time : 5 * 60,
            max_delete_per_iteration: 100
        };
        return ret;
    }

    #[allow(dead_code)]
    pub fn access(&mut self,url: &str) -> HitMiss {
        let mutex = self.mutex.clone();
        let lock = mutex.lock().unwrap();

        let is_some = self.url_map.get(url).is_some();
        if is_some {
            // println!("{:?}", data);
            let state = self.update(url);
            return state;

        } else {
            self.push(url);
        }
        // thread::sleep_ms(1000);

        HitMiss::Miss
    }

    fn push(&mut self, url: &str){
        let data = TimeUrl::new(url.to_owned());

        self.urls.insert(data.time, url.to_owned());
        self.url_map.insert(url.to_owned(),data);
    }

    fn update(&mut self, url: &str) -> HitMiss {
        let mut value = self.url_map.get_mut(url);
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
        let mutex = self.mutex.clone();
        let lock = mutex.lock().unwrap();

        let mut url_data = self.url_map.get_mut(url);
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


    fn persist(&self) -> Result<(),()>{
        // for url in self.url_map{
        //
        // }
        Err(())
    }

    fn load() -> Result<CacheController,()> {
        Err(())
    }

    fn sweep(&mut self, file_deleter : &Sender<String>) -> usize {

        if self.soft_limit_passed() {
            self.sweep_one_level(self.max_delete_per_iteration)
        }
        self.sweep_time
    }
    fn sweep_one_level(&mut self,num: usize){
        let mut key: Option<u64> = None ;
        let mut url: Option<String> = None;

        for i in 0..num:
        {
            {
                let t = self.urls.iter().nth(0);
                if let Some(u) = t {
                    key = Some(*u.0);
                    url = Some(u.1.to_owned());
                }
            }
            if key.is_some() {
                let url_key = url.as_ref().unwrap();
                self.urls.remove(key.as_ref().unwrap());
                let time_url = self.url_map.get(url_key).unwrap();
                let size = time_url.size;
                self.url_map.remove(url_key);

                self.size -= size;
                if ! self.soft_limit_passed() {break}
            }
        }
    }

    pub fn soft_limit_passed(&self) -> bool {
        self.size  > (self.size_limit as f64 * self.soft_limit_ratio) as usize
    }

    pub fn hard_limit_passed(&self) -> bool {
        self.size > self.size_limit
    }

}

impl Drop for CacheController {
    fn drop(&mut self){

    }
}

struct Sweeper {
    sweeper_handle : JoinHandle<()>,
    file_deleter_handle : JoinHandle<()>,
}

impl Sweeper {
    fn new(cache_controller :Mutex<CacheController>) -> Sweeper {
        let (tx,_rx) = channel();

        let sweeper_handle = thread::spawn(move ||{
                let delete_channel = tx;
                loop {
                    let mut mutex = cache_controller.lock().unwrap();
                    let delay = mutex.sweep(&delete_channel);
                    drop(mutex);
                    thread::sleep(Duration::from_secs(delay as u64))
                }
            }
        );


        let file_deleter_handle = thread::spawn(move ||{
                let rx = _rx;
                while let Ok(message) = rx.try_recv() {
                    remove_file(message);
                }

            }
        );

        return Sweeper{
            sweeper_handle : sweeper_handle,
            file_deleter_handle : file_deleter_handle
        }
    }

    fn join(&mut self){
        self.file_deleter_handle.join();
        self.sweeper_handle.join();
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
