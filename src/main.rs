extern crate iron;
#[macro_use] extern crate hyper;
use std::sync::{Mutex,Arc};
use iron::prelude::*;
use iron::status;
use iron::BeforeMiddleware;
use iron::typemap::Key;
mod cache_controller;
use cache_controller::*;

mod download;
use download::*;
// use downloader::Downloader;

static base_url: &'static str = "https://divar.ir";
static base_dir: &'static str = "/home/navid/cache";



header! {
    (X_Accel_Redirect,"X-Accel-Redirect") => [String]
}




fn handler(req: &mut Request)-> IronResult<Response> {

    // let base_url = String::from("");
    // let base_dir = String::from("");
    let url = req.url.path.iter()
             .fold(String::new(),|a,b| a + "/" + b);


    let xaccel_uri : String = format!("files{}",url);

    {
        let mutex = req.extensions.get::<SharedCache>().unwrap();
        let mut cache_controller = mutex.lock().unwrap();
        match cache_controller.access(url.as_ref()) {
            HitMiss::Downloading => {/*wait!!!*/},
            HitMiss::Hit => {
                println!("hit {:?}",url);
                return xaccel_redirect(xaccel_uri)},
            _ => {println!("miss {:?}", url);}
        }
    }
    println!("downloading");

    let download_url = base_url.to_string() + url.as_ref();
    let download_save_file :String = base_dir.to_string() + url.as_ref();

    {
        let download_result =  download_file(download_url.as_ref(),download_save_file.as_ref());
        let mutex = req.extensions.get::<SharedCache>().unwrap();
        let mut cache_controller = mutex.lock().unwrap();
        if let Ok(size) = download_result{
            cache_controller.download_done(url.as_ref(),size);
            cache_controller.dump();
        } else {
            cache_controller.remove(url.as_ref());
            println!("fatal error");
        }
    }

    // let bytes =  xaccel_uri.as_mut_vec();

    // let () = v;
    // let h = Headers::new();
    // h.set(X_Accel_Redirect("/dasdsad/sadsad".to_owned()));
    println!("{:?}", xaccel_uri);
    return xaccel_redirect(xaccel_uri);
}

fn xaccel_redirect(internal_url: String) -> IronResult<Response>{
    let mut res = Response::with(status::Ok);
    res.headers.set(X_Accel_Redirect(internal_url));
    Ok(res)
}

struct SharedCache;
impl Key for SharedCache {
    type Value = Arc<Mutex<CacheController>>;
}

struct CacheMiddleware{
    cache_controller : Arc<Mutex<CacheController>>
}

impl BeforeMiddleware for CacheMiddleware {
    fn before(&self, req: &mut Request) -> IronResult<()>{
        req.extensions.insert::<SharedCache>(self.cache_controller.clone());
        Ok(())
    }
}

fn main(){
    // let mut cache_controller = CacheController::new();
    let cache_controller = Arc::new(Mutex::new(CacheController::new()));
    let sweeper = Sweeper::new(cache_controller.clone(),base_dir.to_string());
    let mut chain = Chain::new(handler);
    // let write : (Write<AccessCache>,Write<AccessCache>) = Write::<AccessCache>::both(cache_controller);
    // let t : Write<AccessCache> = write.0;
    // let data = t.data.clone();
    // chain.link(write);
    chain.link_before(CacheMiddleware{cache_controller: cache_controller});

    // chain.link((cache_controller.clone(),cache_controller.clone()));
    // let mut sweeper = Sweeper::new(chain);
    Iron::new(chain).http("0.0.0.0:9999").unwrap();
    sweeper.join();
}
