pub mod cmd {
    pub mod backup;
    pub mod common;
    pub mod restore;
}

pub mod data {
    pub mod config;
    pub mod backup {
        include!(concat!(env!("OUT_DIR"), "/freebck.data.backup.rs"));
    }
}

pub mod storage;

pub mod util {
    pub mod fs;
    pub mod hash;
    pub mod time;
}

pub mod constants;
