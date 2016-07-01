#[macro_use] extern crate hyper;
#[macro_use] extern crate iron;
extern crate rustc_serialize;
extern crate persistent;

use std::sync::{Mutex,Arc};

use iron::prelude::*;
use iron::status;
use persistent::Write;
use iron::typemap::Key;

mod cache_controller;
use cache_controller::*;

mod download;
use download::*;
// use downloader::Downloader;

header! {
    (X_Accel_Redirect,"X-Accel-Redirect") => [String]
}

#[derive(Clone,Copy)]
struct AccessCache;
impl Key for AccessCache {
    type Value = CacheController;
}


fn handler(req: &mut Request)-> IronResult<Response> {

    let base_url = String::from("https://divar.ir");
    let base_dir = String::from("/home/navid/cache");
    let url = req.url.path.iter()
             .fold(String::new(),|a,b| a + "/" + b);


    let xaccel_uri : String = format!("files{}",url);

    {
        let mutex = req.get::<Write<AccessCache>>().unwrap();
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

    let download_url = base_url + url.as_ref();
    let download_save_file :String = base_dir + url.as_ref();

    {
        let download_result =  download_file(download_url.as_ref(),download_save_file.as_ref());
        let mutex = req.get::<Write<AccessCache>>().unwrap();
        let mut cache_controller = mutex.lock().unwrap();
        if let Ok(size) = download_result{
            cache_controller.download_done(url.as_ref(),size);
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

fn main(){
    let cache_controller = CacheController::new();
    // let cache_controller = Arc::new(Mutex::new(CacheController::new()));
    let mut chain = Chain::new(handler);
    chain.link(Write::<AccessCache>::both(cache_controller));
    // chain.link((cache_controller.clone(),cache_controller.clone()));
    Iron::new(chain).http("0.0.0.0:9999").unwrap();

}
