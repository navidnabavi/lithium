use std::fs::File;
use std::fs::create_dir_all;
use std::io::BufWriter;
use std::io::prelude::*;
use std::path::Path;
use std::fs::remove_file;
use hyper::client::*;

pub fn download_file(url: &str,filename: &str) -> Result<usize,()> {
    let  client = Client::new();

    let path = Path::new(filename);

    let parent: &Path = path.parent().unwrap();

    create_dir_all(parent.to_str().unwrap()).unwrap();
    
    let mut res = client.get(url).send().unwrap();
    let file = match File::create(filename.to_owned()) {
        Ok(file) => file,
        Err(why) => panic!("Error opening file: {} {}",filename ,why)
    };

    let mut writer = BufWriter::new(file);
    
    let mut buffer :[u8;10240] = [0;10240];
    let mut size_all = 0usize;
    loop {
        match res.read(&mut buffer)
        {
            Ok(0) =>
            {
                writer.flush().unwrap();
                drop(writer);
                return Ok(size_all);
            }
            Ok(size)=> {
                writer.write(&buffer[0..size]);
                println!("{:?}", size);
                size_all += size;
            },
            Err(err) => {
                drop(writer);
                remove_file(filename);
                println!("{:?}",err );
                return Err(());
            }
        }
    }

}
