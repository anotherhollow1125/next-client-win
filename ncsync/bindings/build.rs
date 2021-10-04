fn main() {
    windows::build! {
        Windows::Win32::{
            Foundation::*,
            UI::{WindowsAndMessaging::*, Shell::*},
            System::{
                LibraryLoader::{
                    GetModuleHandleA,
                },
                DataExchange::*,
            },
        },
    };
}
