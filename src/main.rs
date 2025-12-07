use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_itunes_library::ITLibrary;

struct Music {
  itl: Retained<ITLibrary>,
}

impl Music {
  fn new() -> Self {
    let version = NSString::from_str("1");
    let itl = unsafe { ITLibrary::libraryWithAPIVersion_error(&version) }.expect("Failed to load library");
    Music { itl }
  }
}

fn main() {
  let lists = ["eatmos", "ebup", "edrive", "epeak", "ebang", "ebdown"];
  let version = NSString::from_str("1");
  let itl = unsafe { ITLibrary::libraryWithAPIVersion_error(&version) }.expect("Failed to load library");
  let playlists = unsafe { itl.allPlaylists() };
  for list in lists.into_iter() {
    let version = NSString::from_str(list);
    let playlist = unsafe { playlists.iter().find(|pl| pl.name().isEqualToString(&*version)) };
    unsafe {
      println!("Playlist: {:?}", playlist.unwrap().name());
    }
  }
  // playlists.iter().for_each(|item| unsafe {
  //   println!("{:?}", item.name());
  // });
}
