mod window_info_repository;
mod window_info_type;
mod load_window_info;
mod from_window_info_repository;

pub use from_window_info_repository::FromWindowInfoRepository;
pub use window_info_repository::WindowInfoRepository;
pub use window_info_type::WindowInfoType;
pub use crate::load_window_info_repo;
#[doc(hidden)]
pub use load_window_info::inject_window_info_template;
