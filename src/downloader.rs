// use std::collections::{LinkedList};
// use std::thread;
// use std::thread::{JoinHandle,Thread};
// use td::sync::mpsc::*;
// use std::sync::{Arc,Mutex};
// use std::time::Duration;
// use std::path::PathBuf;
// extern crate hyper;
// use hyper::Client;
// //
// // struct DownloadFile {
// //     url: String,
// //     downloaded: u64
// // }
//
//
// // static mut base_url: &'static str = "";
//
// #[derive(Debug)]
// enum WorkerMessages {
//     Done(String,u64),
//     Error(String,u16),
// }
//
//
// struct Worker {
//     thread : JoinHandle<()>,
//     id: usize
// }
//
// type JobList = Arc<Mutex<LinkedList<String>>>;
//
// impl Worker {
//     fn spawn(id: usize,tx:Sender<WorkerMessages>, list: JobList, base_url: String-> Worker {
//         let l = list.clone();
//         let th =  thread::spawn(move ||{
//             // thread::current().
//             loop {
//
//                 let  pre_lock =  l.lock();
//                 if pre_lock.is_err() {
//                     panic!("Lost Lock in thread");
//                 }
//                 let mut lock = pre_lock.unwrap();
//
//                 if let Some(url) = lock.pop_front() {
//                     drop(lock);
//
//                     let mut path = PathBuf::from(base_url.to_owned());
//                     path.push(url.to_owned());
//                     let _url = path.to_str().unwrap();
//                     Worker::download(path);
//
//                     if tx.send(WorkerMessages::Done(_url.to_owned(),100)).is_err() {
//                         break;
//                     }
//                 }
//                 else {
//                     drop(lock);
//                     thread::sleep(Duration::from_millis(100));
//                 }
//             }
//         });
//
//         return Worker{
//             thread: th,
//             id: id
//         }
//     }
//
//     fn download(url: PathBuf) ->Result<u64,u16> {
//         let client = Client::new();
//
//         if let Ok(res) =  client.get(url.to_str().unwrap().to_owned()).send() {
//             let file = match File::create(save_path) {
//                 Ok(file) => file,
//                 Err(why) => panic!("Error opening file: {} {}",display ,why)
//             };
//             let mut writer = BufWriter::new(file);
//             let mut buffer :[u8;1024] = [0;1024];
//             loop {
//                 match res.read(&mut buffer)
//                 {
//                     Ok(0) =>
//                     {
//                         thread::sleep(Duration::from_millis(100));
//                         // println!("{:?}", res);
//                         writer.flush().unwrap();
//                         break;
//                         // drop(writer);
//                         // drop(file);
//                     }
//                     Ok(size)=> {
//                         // let b_arr = &buffer[0..size];
//                         // let t = buffer.iter().take(size);
//                         writer.write(&buffer[0..size]);
//                         println!("{:?}",size );
//                     },
//                     Err(_) => {
//                         break;
//                     }
//                 }
//             }
//             return Ok();
//         }
//     }
// }
//
// //
// //
// // impl DownloadFile {
// //
// // }
//
//
// pub struct Downloader {
//     job_list: JobList,
//     threads : Vec<Worker>,
//     manager : JoinHandle<()>,
//     tx : Sender<WorkerMessages>,
//     // base_url: String
//     // rx : Receiver<WorkerMessages>
//     // die : Arc< Mutex<bool> >
// }
//
// impl Downloader {
//     pub fn new(workers: usize, base_url: String) -> Downloader {
//
//         let (tx,rx) = channel();
//         let manager_thread = thread::spawn(move ||{
//
//             while let Ok(msg) = rx.recv() {
//                 match msg {
//                     WorkerMessages::Error(_,1000) => break,
//                     _ => println!("{:?}", msg)
//                 }
//             }
//             println!("Exiting");
//         });
//
//         let mut obj = Downloader {
//             job_list : Arc::new(Mutex::new(LinkedList::new() )),
//             threads: Vec::with_capacity(workers),
//             tx : tx,
//             // rx: rx,
//             // die : Arc::new(),
//             // base_url : base_url,
//             manager: manager_thread
//         };
//         for i in 0..workers {
//             obj.threads.push(Worker::spawn(i,obj.tx.clone(), obj.job_list.clone(),base_url.to_owned()));
//         }
//         return obj;
//     }
//
//
//
//     pub fn push(&mut self,url : &str) {
//         let t = self.job_list.lock();
//         if t.is_err() {
//             panic!("Lost Lock in push")
//         }
//         let mut lock = t.unwrap();
//         lock.push_back(url.to_owned());
//     }
//
//     pub fn wait(self){
//         self.tx.send(WorkerMessages::Error("EXIT".to_owned(),1000)).unwrap();
//         drop(self.tx);
//         self.manager.join();
//     }
//
// }
// //
// // pub unsafe fn set_base_url(url : &'static str){
// //     base_url = &url;
// // }
