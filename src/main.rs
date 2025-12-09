#![windows_subsystem = "windows"]

use eframe::egui;
use rfd::FileDialog;
use std::collections::HashMap;
use std::fs;

use std::path::PathBuf;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

const BF6_APP_ID: &str = "2807960";

#[derive(Clone)]
struct Language {
    name: &'static str,
    miles_lang: &'static str,
}

fn get_languages() -> HashMap<&'static str, Language> {
    let mut langs = HashMap::new();
    langs.insert("en", Language { name: "英语 (English)", miles_lang: "english" });
    langs.insert("ja", Language { name: "日语 (Japanese)", miles_lang: "japanese" });
    langs.insert("cn", Language { name: "中文 (Chinese)", miles_lang: "chinese" });
    langs.insert("de", Language { name: "德语 (German)", miles_lang: "german" });
    langs.insert("fr", Language { name: "法语 (French)", miles_lang: "french" });
    langs.insert("es", Language { name: "西班牙语 (Spanish)", miles_lang: "spanish" });
    langs.insert("ru", Language { name: "俄语 (Russian)", miles_lang: "russian" });
    langs.insert("ko", Language { name: "韩语 (Korean)", miles_lang: "korean" });
    langs
}

#[derive(Clone, Default)]
struct BackupInfo {
    lang_code: String,
    build_id: String,
}

#[derive(Clone, Default)]
struct SteamInfo {
    game_path: PathBuf,
    build_id: String,
}

struct BF6VoiceSwitcher {
    languages: HashMap<&'static str, Language>,
    lang_codes: Vec<&'static str>,
    selected_lang_idx: usize,
    source_path: String,
    backup_dir: PathBuf,
    available_backups: Vec<BackupInfo>,
    selected_backup_idx: usize,
    status_message: String,
    is_error: bool,
    steam_info: Option<SteamInfo>,
}

impl Default for BF6VoiceSwitcher {
    fn default() -> Self {
        let backup_dir = std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .join("voice_backups");

        let languages = get_languages();
        let lang_codes = vec!["en", "ja", "cn", "de", "fr", "es", "ru", "ko"];

        let mut app = Self {
            languages,
            lang_codes,
            selected_lang_idx: 0,
            source_path: String::new(),
            backup_dir,
            available_backups: Vec::new(),
            selected_backup_idx: 0,
            status_message: String::new(),
            is_error: false,
            steam_info: None,
        };
        
        // 自动检测 Steam
        app.detect_steam();
        app.refresh_backups();
        app
    }
}

impl BF6VoiceSwitcher {
    /// 检测 Steam 安装路径和游戏信息
    fn detect_steam(&mut self) {
        // 常见 Steam 安装路径
        let possible_paths = vec![
            PathBuf::from("C:\\Program Files (x86)\\Steam"),
            PathBuf::from("C:\\Program Files\\Steam"),
            PathBuf::from("D:\\Steam"),
            PathBuf::from("E:\\Steam"),
            PathBuf::from("D:\\Program Files (x86)\\Steam"),
            PathBuf::from("E:\\Program Files (x86)\\Steam"),
        ];

        // 也尝试从注册表读取（简化版，直接检查路径）
        for steam_path in possible_paths {
            if steam_path.join("steam.exe").exists() {
                if let Some(info) = self.parse_steam_info(&steam_path) {
                    self.steam_info = Some(info.clone());
                    self.source_path = info.game_path.join("Data").join("Win32").to_string_lossy().to_string();
                    self.status_message = format!("已自动检测到游戏路径，版本: {}", info.build_id);
                    self.is_error = false;
                    return;
                }
            }
        }
    }

    /// 解析 Steam 信息
    fn parse_steam_info(&self, steam_path: &PathBuf) -> Option<SteamInfo> {
        // 读取 libraryfolders.vdf 获取所有库路径
        let library_folders = self.get_library_folders(steam_path);
        
        // 在所有库中查找 BF6
        for lib_path in library_folders {
            let manifest_path = lib_path.join("steamapps").join(format!("appmanifest_{}.acf", BF6_APP_ID));
            if manifest_path.exists() {
                if let Some((install_dir, build_id)) = self.parse_app_manifest(&manifest_path) {
                    return Some(SteamInfo {
                        game_path: lib_path.join("steamapps").join("common").join(install_dir),
                        build_id,
                    });
                }
            }
        }
        None
    }

    /// 获取所有 Steam 库文件夹
    fn get_library_folders(&self, steam_path: &PathBuf) -> Vec<PathBuf> {
        let mut folders = vec![steam_path.clone()];
        let vdf_path = steam_path.join("steamapps").join("libraryfolders.vdf");
        
        if let Ok(content) = fs::read_to_string(&vdf_path) {
            for line in content.lines() {
                if line.contains("\"path\"") {
                    if let Some(path) = self.extract_vdf_value(line) {
                        let path = PathBuf::from(path.replace("\\\\", "\\"));
                        if path.exists() && !folders.contains(&path) {
                            folders.push(path);
                        }
                    }
                }
            }
        }
        folders
    }

    /// 解析 appmanifest 文件
    fn parse_app_manifest(&self, path: &PathBuf) -> Option<(String, String)> {
        let content = fs::read_to_string(path).ok()?;
        let mut install_dir = String::new();
        let mut build_id = String::new();

        for line in content.lines() {
            if line.contains("\"installdir\"") {
                install_dir = self.extract_vdf_value(line).unwrap_or_default();
            } else if line.contains("\"buildid\"") {
                build_id = self.extract_vdf_value(line).unwrap_or_default();
            }
        }

        if !install_dir.is_empty() && !build_id.is_empty() {
            Some((install_dir, build_id))
        } else {
            None
        }
    }

    /// 从 VDF 行中提取值
    fn extract_vdf_value(&self, line: &str) -> Option<String> {
        let parts: Vec<&str> = line.split('"').collect();
        if parts.len() >= 4 {
            Some(parts[3].to_string())
        } else {
            None
        }
    }

    fn refresh_backups(&mut self) {
        self.available_backups.clear();
        if let Ok(entries) = fs::read_dir(&self.backup_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if self.languages.contains_key(name.as_str()) {
                        // 读取备份信息
                        let info_path = entry.path().join("backup_info.txt");
                        let build_id = if let Ok(content) = fs::read_to_string(&info_path) {
                            content.lines()
                                .find(|l| l.starts_with("build_id="))
                                .map(|l| l.trim_start_matches("build_id=").to_string())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        
                        self.available_backups.push(BackupInfo {
                            lang_code: name,
                            build_id,
                        });
                    }
                }
            }
        }
        self.selected_backup_idx = 0;
    }

    fn get_selected_lang_code(&self) -> &'static str {
        self.lang_codes[self.selected_lang_idx]
    }

    fn get_launch_param(&self) -> String {
        let code = self.get_selected_lang_code();
        if let Some(lang) = self.languages.get(code) {
            format!("+miles_language {}", lang.miles_lang)
        } else {
            String::new()
        }
    }

    /// 递归查找所有匹配的语音文件夹和 .toc 文件，返回 (文件夹列表, toc文件列表)
    fn find_voice_files(&self, root: &PathBuf, lang_code: &str) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let folder_names = [lang_code.to_string(), format!("vo{}", lang_code)];
        let toc_names = [format!("{}.toc", lang_code), format!("vo{}.toc", lang_code)];
        let mut folders = Vec::new();
        let mut toc_files = Vec::new();
        self.find_voice_files_recursive(root, root, &folder_names, &toc_names, &mut folders, &mut toc_files);
        (folders, toc_files)
    }

    fn find_voice_files_recursive(
        &self,
        root: &PathBuf,
        current: &PathBuf,
        folder_names: &[String],
        toc_names: &[String],
        folders: &mut Vec<PathBuf>,
        toc_files: &mut Vec<PathBuf>,
    ) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = path.is_dir() || Self::is_junction(&path);
            
            if is_dir {
                if folder_names.contains(&name) {
                    if let Ok(rel) = path.strip_prefix(root) {
                        folders.push(rel.to_path_buf());
                    }
                } else if !Self::is_junction(&path) {
                    // 只递归普通目录，不递归 Junction
                    self.find_voice_files_recursive(
                        root,
                        &path,
                        folder_names,
                        toc_names,
                        folders,
                        toc_files,
                    );
                }
            } else if toc_names.contains(&name) {
                if let Ok(rel) = path.strip_prefix(root) {
                    toc_files.push(rel.to_path_buf());
                }
            }
        }
    }

    fn backup_files(&mut self) {
        if self.source_path.is_empty() {
            self.status_message = "请先选择语音文件夹！".to_string();
            self.is_error = true;
            return;
        }

        let source = PathBuf::from(&self.source_path);
        if !source.exists() {
            self.status_message = "所选文件夹不存在！".to_string();
            self.is_error = true;
            return;
        }

        let lang_code = self.get_selected_lang_code();
        let target = self.backup_dir.join(lang_code);

        // 递归查找所有语音文件夹和 .toc 文件
        let (voice_folders, toc_files) = self.find_voice_files(&source, lang_code);

        if voice_folders.is_empty() && toc_files.is_empty() {
            self.status_message = format!("未找到语音文件: {} 或 vo{}", lang_code, lang_code);
            self.is_error = true;
            return;
        }

        // 只有 toc 文件时，备份不完整，不执行备份
        if voice_folders.is_empty() {
            let lang_name = self.languages.get(lang_code).map(|l| l.name).unwrap_or(lang_code);
            self.status_message = format!("[!] {} 备份不完整！未找到语音文件夹，已取消备份", lang_name);
            self.is_error = true;
            return;
        }

        // 清理旧备份
        if target.exists() {
            if let Err(e) = fs::remove_dir_all(&target) {
                self.status_message = format!("删除旧备份失败: {}", e);
                self.is_error = true;
                return;
            }
        }

        // 复制所有语音文件夹，保持目录结构
        let options = fs_extra::dir::CopyOptions::new().overwrite(true);
        let mut success = true;
        let mut copied_folders = 0;
        let mut copied_files = 0;

        // 复制文件夹
        for rel_path in &voice_folders {
            let src_folder = source.join(rel_path);
            let dst_parent = target.join(rel_path.parent().unwrap_or(rel_path));
            
            if let Err(e) = fs::create_dir_all(&dst_parent) {
                self.status_message = format!("创建目录失败: {}", e);
                self.is_error = true;
                success = false;
                break;
            }
            
            if let Err(e) = fs_extra::dir::copy(&src_folder, &dst_parent, &options) {
                self.status_message = format!("备份 {} 失败: {}", rel_path.display(), e);
                self.is_error = true;
                success = false;
                break;
            }
            copied_folders += 1;
        }

        // 复制 .toc 文件
        if success {
            for rel_path in &toc_files {
                let src_file = source.join(rel_path);
                let dst_file = target.join(rel_path);
                
                if let Some(parent) = dst_file.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        self.status_message = format!("创建目录失败: {}", e);
                        self.is_error = true;
                        success = false;
                        break;
                    }
                }
                
                if let Err(e) = fs::copy(&src_file, &dst_file) {
                    self.status_message = format!("备份 {} 失败: {}", rel_path.display(), e);
                    self.is_error = true;
                    success = false;
                    break;
                }
                copied_files += 1;
            }
        }

        if success {
            // 保存备份信息
            let build_id = self.steam_info.as_ref().map(|s| s.build_id.clone()).unwrap_or_default();
            let folders_str: Vec<String> = voice_folders.iter().map(|p| p.to_string_lossy().to_string()).collect();
            let files_str: Vec<String> = toc_files.iter().map(|p| p.to_string_lossy().to_string()).collect();
            let info_content = format!("build_id={}\nlang_code={}\nfolders={}\ntoc_files={}\n", 
                build_id, lang_code, folders_str.join(";"), files_str.join(";"));
            let _ = fs::write(target.join("backup_info.txt"), info_content);
            
            let lang_name = self.languages.get(lang_code).map(|l| l.name).unwrap_or(lang_code);
            self.status_message = format!("{} 备份完成！({} 个文件夹, {} 个toc文件, 版本: {})", 
                lang_name, copied_folders, copied_files, build_id);
            self.is_error = false;
            self.refresh_backups();
        }
    }

    fn restore_files(&mut self) {
        if self.source_path.is_empty() {
            self.status_message = "请先选择游戏语音文件夹！".to_string();
            self.is_error = true;
            return;
        }

        if self.available_backups.is_empty() {
            self.status_message = "没有可用的备份！".to_string();
            self.is_error = true;
            return;
        }

        let backup_info = self.available_backups[self.selected_backup_idx].clone();
        let backup_path = self.backup_dir.join(&backup_info.lang_code);
        let target = PathBuf::from(&self.source_path);

        if !backup_path.exists() {
            self.status_message = "备份文件不存在！".to_string();
            self.is_error = true;
            return;
        }

        // 版本检查 - 不匹配时阻止恢复
        if let Some(steam_info) = &self.steam_info {
            if !backup_info.build_id.is_empty() && backup_info.build_id != steam_info.build_id {
                self.status_message = format!(
                    "[!] 版本不匹配！备份: {}, 当前: {}\n请先删除游戏中的语音文件，然后重新执行所有步骤",
                    backup_info.build_id, steam_info.build_id
                );
                self.is_error = true;
                return;
            }
        }

        // 递归查找备份中的所有语音文件夹和 .toc 文件
        let (voice_folders, toc_files) = self.find_voice_files(&backup_path, &backup_info.lang_code);
        let mut success = true;
        let mut restored_folders = 0;
        let mut restored_files = 0;

        // 使用 Junction 链接文件夹
        for rel_path in &voice_folders {
            let src_folder = backup_path.join(rel_path);
            let dst_parent = target.join(rel_path.parent().unwrap_or(rel_path));
            let dst_folder = target.join(rel_path);
            
            // 先删除目标
            if Self::is_junction(&dst_folder) {
                let _ = Self::remove_junction(&dst_folder);
            }
            
            // 创建目标父目录
            if let Err(e) = fs::create_dir_all(&dst_parent) {
                self.status_message = format!("创建目录失败: {}", e);
                self.is_error = true;
                success = false;
                break;
            }
            
            // 创建 Junction
            if let Err(e) = Self::create_junction(&src_folder, &dst_folder) {
                self.status_message = format!("创建链接 {} 失败: {}", rel_path.display(), e);
                self.is_error = true;
                success = false;
                break;
            }
            restored_folders += 1;
        }

        // 复制 .toc 文件
        if success {
            for rel_path in &toc_files {
                let src_file = backup_path.join(rel_path);
                let dst_file = target.join(rel_path);
                
                if let Some(parent) = dst_file.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        self.status_message = format!("创建目录失败: {}", e);
                        self.is_error = true;
                        success = false;
                        break;
                    }
                }
                
                if let Err(e) = fs::copy(&src_file, &dst_file) {
                    self.status_message = format!("恢复 {} 失败: {}", rel_path.display(), e);
                    self.is_error = true;
                    success = false;
                    break;
                }
                restored_files += 1;
            }
        }

        if success && (restored_folders > 0 || restored_files > 0) {
            let lang = self.languages.get(backup_info.lang_code.as_str());
            let lang_name = lang.map(|l| l.name).unwrap_or(&backup_info.lang_code);
            let miles_lang = lang.map(|l| l.miles_lang).unwrap_or("");
            self.status_message = format!("语音已链接为 {}！({} 个链接, {} 个toc文件)\n请添加启动项: +miles_language {}", 
                lang_name, restored_folders, restored_files, miles_lang);
            self.is_error = false;
        } else if restored_folders == 0 && restored_files == 0 {
            self.status_message = "备份中没有找到语音文件".to_string();
            self.is_error = true;
        }
    }

    /// 创建 Junction
    fn create_junction(src: &PathBuf, dst: &PathBuf) -> Result<(), String> {
        let output = Command::new("cmd")
            .args(["/C", "mklink", "/J", &dst.to_string_lossy(), &src.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| e.to_string())?;
        
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// 检查路径是否为 Junction
    fn is_junction(path: &PathBuf) -> bool {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        
        if let Ok(metadata) = fs::symlink_metadata(path) {
            (metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
        } else {
            false
        }
    }

    /// 删除 Junction
    fn remove_junction(path: &PathBuf) -> Result<(), std::io::Error> {
        Command::new("cmd")
            .args(["/C", "rmdir", &path.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        Ok(())
    }

    fn check_version_match(&self) -> Option<(String, String)> {
        if self.available_backups.is_empty() {
            return None;
        }
        
        let backup = &self.available_backups[self.selected_backup_idx];
        if let Some(steam) = &self.steam_info {
            if !backup.build_id.is_empty() && backup.build_id != steam.build_id {
                return Some((backup.build_id.clone(), steam.build_id.clone()));
            }
        }
        None
    }

    /// 删除游戏目录中指定语言的所有语音文件夹和 .toc 文件（递归）
    fn delete_voice_files(&mut self) {
        if self.source_path.is_empty() {
            self.status_message = "请先选择语音文件夹！".to_string();
            self.is_error = true;
            return;
        }

        let source = PathBuf::from(&self.source_path);
        if !source.exists() {
            self.status_message = "所选文件夹不存在！".to_string();
            self.is_error = true;
            return;
        }

        let lang_code = self.get_selected_lang_code();
        
        // 递归查找所有语音文件夹和 .toc 文件
        let (voice_folders, toc_files) = self.find_voice_files(&source, lang_code);
        
        if voice_folders.is_empty() && toc_files.is_empty() {
            self.status_message = format!("未找到语音文件: {} 或 vo{}", lang_code, lang_code);
            self.is_error = true;
            return;
        }

        let mut deleted_folders = 0;
        let mut deleted_files = 0;

        // 删除 Junction
        for rel_path in &voice_folders {
            let folder_path = source.join(rel_path);
            if Self::is_junction(&folder_path) {
                if let Err(e) = Self::remove_junction(&folder_path) {
                    self.status_message = format!("删除 {} 失败: {}", rel_path.display(), e);
                    self.is_error = true;
                    return;
                }
                deleted_folders += 1;
            }
        }

        // 删除 .toc 文件
        for rel_path in &toc_files {
            let file_path = source.join(rel_path);
            if file_path.exists() {
                if let Err(e) = fs::remove_file(&file_path) {
                    self.status_message = format!("删除 {} 失败: {}", rel_path.display(), e);
                    self.is_error = true;
                    return;
                }
                deleted_files += 1;
            }
        }

        let lang_name = self.languages.get(lang_code).map(|l| l.name).unwrap_or(lang_code);
        self.status_message = format!("{} 语音文件已删除！({} 个文件夹, {} 个toc文件)", 
            lang_name, deleted_folders, deleted_files);
        self.is_error = false;
    }

    /// 删除备份
    fn delete_backup(&mut self) {
        if self.available_backups.is_empty() {
            self.status_message = "没有可删除的备份！".to_string();
            self.is_error = true;
            return;
        }

        let backup_info = self.available_backups[self.selected_backup_idx].clone();
        let backup_path = self.backup_dir.join(&backup_info.lang_code);

        if backup_path.exists() {
            if let Err(e) = fs::remove_dir_all(&backup_path) {
                self.status_message = format!("删除备份失败: {}", e);
                self.is_error = true;
                return;
            }
        }

        let lang_name = self.languages.get(backup_info.lang_code.as_str()).map(|l| l.name).unwrap_or(&backup_info.lang_code);
        self.status_message = format!("{} 备份已删除！", lang_name);
        self.is_error = false;
        self.refresh_backups();
    }
}


impl eframe::App for BF6VoiceSwitcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("战地6 语音切换工具");
            ui.add_space(5.0);

            // Steam 状态
            ui.horizontal(|ui| {
                if let Some(steam) = &self.steam_info {
                    ui.label(egui::RichText::new("[OK] Steam 已连接").color(egui::Color32::GREEN));
                    ui.label(format!("| 游戏版本: {}", steam.build_id));
                } else {
                    ui.label(egui::RichText::new("[!] 未检测到 Steam/游戏").color(egui::Color32::YELLOW));
                    if ui.button("重新检测").clicked() {
                        self.detect_steam();
                    }
                }
            });

            ui.add_space(5.0);
            ui.separator();
            ui.add_space(5.0);

            // 步骤1
            ui.group(|ui| {
                ui.label(egui::RichText::new("步骤1: 准备工作").strong());
                ui.label("请先在 Steam 中将战地6切换到您想要使用的语音语言：");
                ui.label("右键战地6 -> 属性 -> 语言 -> 选择语言并等待下载完成");
            });

            ui.add_space(5.0);

            // 步骤2
            ui.group(|ui| {
                ui.label(egui::RichText::new("步骤2: 选择要使用的语音语言").strong());
                ui.horizontal_wrapped(|ui| {
                    for (idx, code) in self.lang_codes.iter().enumerate() {
                        if let Some(lang) = self.languages.get(*code) {
                            if ui.selectable_label(self.selected_lang_idx == idx, lang.name).clicked() {
                                self.selected_lang_idx = idx;
                                if let Some(backup_idx) = self.available_backups.iter().position(|b| b.lang_code == *code) {
                                    self.selected_backup_idx = backup_idx;
                                }
                            }
                        }
                    }
                });
            });

            ui.add_space(5.0);

            // 步骤3
            ui.group(|ui| {
                ui.label(egui::RichText::new("步骤3: 选择语音文件夹").strong());
                ui.label(egui::RichText::new("路径: ...\\Battlefield 6\\Data\\Win32").weak());
                
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.source_path).desired_width(420.0));
                    if ui.button("浏览").clicked() {
                        if let Some(path) = FileDialog::new().pick_folder() {
                            self.source_path = path.to_string_lossy().to_string();
                        }
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("备份语音文件").clicked() {
                        self.backup_files();
                    }
                    if ui.button("删除游戏语音").clicked() {
                        self.delete_voice_files();
                    }
                });
            });

            ui.add_space(5.0);

            // 步骤4
            ui.group(|ui| {
                ui.label(egui::RichText::new("步骤4: 恢复语音文件").strong());
                ui.label("切换到想使用的文本语言后，选择要恢复的语音：");
                
                // 版本警告
                if let Some((backup_ver, current_ver)) = self.check_version_match() {
                    ui.label(egui::RichText::new(format!("[!] 版本不匹配: 备份({}) != 当前({})", backup_ver, current_ver))
                        .color(egui::Color32::RED));
                    ui.label(egui::RichText::new("请先删除游戏语音，再重新执行所有步骤").small());
                }
                
                ui.horizontal(|ui| {
                    ui.label("选择语音:");
                    egui::ComboBox::from_id_salt("backup_select")
                        .selected_text(if self.available_backups.is_empty() {
                            "无备份".to_string()
                        } else {
                            let info = &self.available_backups[self.selected_backup_idx];
                            let name = self.languages.get(info.lang_code.as_str()).map(|l| l.name).unwrap_or(&info.lang_code);
                            if info.build_id.is_empty() {
                                name.to_string()
                            } else {
                                format!("{} (v{})", name, info.build_id)
                            }
                        })
                        .show_ui(ui, |ui| {
                            for (idx, info) in self.available_backups.iter().enumerate() {
                                let name = self.languages.get(info.lang_code.as_str()).map(|l| l.name).unwrap_or(&info.lang_code);
                                let label = if info.build_id.is_empty() {
                                    name.to_string()
                                } else {
                                    format!("{} (v{})", name, info.build_id)
                                };
                                if ui.selectable_label(self.selected_backup_idx == idx, label).clicked() {
                                    self.selected_backup_idx = idx;
                                }
                            }
                        });
                    
                    if ui.button("恢复语音").clicked() {
                        self.restore_files();
                    }
                    if ui.button("删除备份").clicked() {
                        self.delete_backup();
                    }
                    if ui.button("刷新").clicked() {
                        self.refresh_backups();
                    }
                });
            });

            ui.add_space(5.0);

            // 步骤5
            ui.group(|ui| {
                ui.label(egui::RichText::new("步骤5: Steam 启动项").strong());
                ui.label("右键战地6 -> 属性 -> 通用 -> 启动选项，添加以下参数：");
                
                let param = self.get_launch_param();
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut param.clone()).desired_width(250.0));
                    if ui.button("复制到剪贴板").clicked() {
                        ctx.copy_text(param.clone());
                        self.status_message = "已复制到剪贴板！".to_string();
                        self.is_error = false;
                    }
                });
            });

            ui.add_space(10.0);

            // 状态消息
            if !self.status_message.is_empty() {
                let color = if self.is_error {
                    egui::Color32::RED
                } else {
                    egui::Color32::GREEN
                };
                ui.label(egui::RichText::new(&self.status_message).color(color));
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([620.0, 550.0])
            .with_resizable(false),
        ..Default::default()
    };
    
    eframe::run_native(
        "BF6 Voice Switcher",
        options,
        Box::new(|cc| {
            // 加载中文字体
            let mut fonts = egui::FontDefinitions::default();
            
            if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
                fonts.font_data.insert(
                    "msyh".to_owned(),
                    egui::FontData::from_owned(font_data).into(),
                );
                
                fonts.families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "msyh".to_owned());
                    
                fonts.families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .insert(0, "msyh".to_owned());
            }
            
            cc.egui_ctx.set_fonts(fonts);
            
            Ok(Box::new(BF6VoiceSwitcher::default()))
        }),
    )
}
