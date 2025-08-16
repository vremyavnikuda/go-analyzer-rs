use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Рекурсивно ищет все файлы с заданным расширением в указанной директории
fn find_files_with_extension(
    dir: &Path,
    extension: &str,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                find_files_with_extension(&path, extension, files)?;
            } else if let Some(ext) = path.extension() {
                if ext == extension {
                    files.push(path);
                }
            }
        }
    }
    Ok(())
}

/// Ищет конкретные файлы по имени в указанной директории
fn find_specific_files(dir: &Path, filenames: &[&str], files: &mut Vec<PathBuf>) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name() {
                    if let Some(name_str) = name.to_str() {
                        if filenames.contains(&name_str) {
                            files.push(path);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Собирает все .rs, .ts файлы и конфигурационные файлы из указанных директорий и объединяет их в один файл
fn collect_and_concatenate_files(
    src_dirs: &[&str],
    extensions: &[&str],
    output_file: &str,
) -> io::Result<()> {
    let mut all_files = Vec::new();

    // Ищем файлы по всем директориям и расширениям
    for dir in src_dirs {
        for ext in extensions {
            find_files_with_extension(Path::new(dir), ext, &mut all_files)?;
        }
    }

    // Ищем конкретные файлы в корневой директории проекта
    let root_dir = Path::new(r"C:\repository\go-analyzer-rs");
    find_specific_files(root_dir, &["Cargo.toml"], &mut all_files)?;

    // Ищем package.json в директории vscode
    let vscode_dir = Path::new(r"C:\repository\go-analyzer-rs\vscode");
    find_specific_files(vscode_dir, &["package.json"], &mut all_files)?;

    // Сортируем файлы для более предсказуемого порядка
    all_files.sort();

    // Открываем файл для записи
    let output = File::create(output_file)?;
    let mut writer = BufWriter::new(output);

    for file_path in &all_files {
        // Пишем заголовок с путем к файлу
        writeln!(writer, "// --- FILE: {} ---", file_path.display())?;

        // Читаем и записываем содержимое файла
        let content = fs::read_to_string(file_path)?;
        writer.write_all(content.as_bytes())?;
        writeln!(writer, "\n// --- END FILE: {} ---\n", file_path.display())?;
    }

    writer.flush()?;
    Ok(())
}

fn main() {
    println!("Code Collector: собираем .rs, .ts файлы и конфигурационные файлы в один файл...");

    // Пути к директориям для поиска
    let src_dirs = [
        r"C:\repository\go-analyzer-rs\src",
        r"C:\repository\go-analyzer-rs\vscode\src",
    ];
    // Расширения файлов для поиска
    let extensions = ["rs", "ts"];
    // Имя итогового файла
    let output_file = "collected_code.txt";

    match collect_and_concatenate_files(&src_dirs, &extensions, output_file) {
        Ok(_) => println!("Все файлы успешно собраны в '{}'", output_file),
        Err(e) => eprintln!("Ошибка при сборке файлов: {}", e),
    }
}
