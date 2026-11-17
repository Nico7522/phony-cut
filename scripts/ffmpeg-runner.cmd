@echo off
rem Cargo runner (see .cargo\config.toml): makes the FFmpeg DLLs from
rem vendor\ffmpeg\bin loadable by `cargo run` / `cargo test` binaries, whose
rem location inside target\ is not predictable.
set "PATH=%~dp0..\vendor\ffmpeg\bin;%PATH%"
%*
