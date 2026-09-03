//! End User License Agreement (EULA) and software terms for supported benchmarks.

/// Returns the official EULA / License text for the specified benchmark ID.
pub fn get_eula_for_benchmark(id: &str) -> &'static str {
    match id {
        "7zip" => include_str!("../resources/eula/7zip.txt"),
        "blender" => include_str!("../resources/eula/blender.txt"),
        "cray" => include_str!("../resources/eula/cray.txt"),
        "ffmpeg" => include_str!("../resources/eula/ffmpeg.txt"),
        "fio" => include_str!("../resources/eula/fio.txt"),
        "geekbench" => include_str!("../resources/eula/geekbench.txt"),
        "gravitymark" => include_str!("../resources/eula/gravitymark.txt"),
        "llama-bench" => include_str!("../resources/eula/llama-bench.txt"),
        "stream" => include_str!("../resources/eula/stream.txt"),
        "unigine" => include_str!("../resources/eula/unigine.txt"),
        "ycruncher" => include_str!("../resources/eula/ycruncher.txt"),
        _ => "Standard Benchmark End User License Agreement & Terms of Use.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_benchmarks_have_eula() {
        let benchmarks = [
            "7zip",
            "blender",
            "cray",
            "ffmpeg",
            "fio",
            "geekbench",
            "gravitymark",
            "llama-bench",
            "stream",
            "unigine",
            "ycruncher",
        ];
        for b in benchmarks {
            let eula = get_eula_for_benchmark(b);
            assert!(!eula.is_empty(), "EULA should not be empty for {}", b);
            assert!(
                eula.len() > 100,
                "EULA should contain detailed terms for {}",
                b
            );
        }
    }
}
