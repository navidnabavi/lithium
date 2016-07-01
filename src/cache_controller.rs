extern crate time;
use std::sync::{Mutex,Arc};
use std::collections::{LinkedList,HashMap};
use std::u64;
use std::usize;

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
    urls: LinkedList<String>,
    url_map: HashMap<String , TimeUrlRef>,
    size : usize,
    mutex: Arc<Mutex<bool>>,
    size_limit: usize,
    time_limit: u64
}

impl CacheController{

    pub fn new() -> CacheController{
        CacheController {
            urls: LinkedList::new(),
            url_map : HashMap::new(),
            size: 0,
            mutex: Arc::new(Mutex::new(false)),
            time_limit : u64::MAX,
            size_limit: usize::MAX
        }
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

        self.urls.push_back(url.to_owned());
        self.url_map.insert(url.to_owned(),data);
    }

    fn update(&mut self, url: &str) -> HitMiss {
        let value = self.url_map.get(url);
        if value.is_some() {
            match value.unwrap().size {
                0 => return HitMiss::Downloading,
                _ => return HitMiss::Hit,
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


    pub fn persist(&self){

    }

    pub fn load() -> Result<CacheController,()> {
        Err(())
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
