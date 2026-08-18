#[cfg(target_os = "macos")]
mod macos_test {
    use image::ImageFormat;
    use objc2::{
        msg_send,
        rc::{autoreleasepool, Retained},
        runtime::ProtocolObject,
        ClassType, MainThreadMarker,
    };
    use objc2_app_kit::{
        NSApplication, NSPasteboard, NSPasteboardItem, NSPasteboardTypeFileURL,
        NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardWriting,
    };
    use objc2_foundation::{NSArray, NSData, NSString};

    pub fn run() {
        let ran = autoreleasepool(|_| {
            let marker = MainThreadMarker::new().expect("integration test must run on main thread");
            let _application = NSApplication::sharedApplication(marker);
            let name =
                NSString::from_str(&format!("com.easyclipboard.tests.{}", uuid::Uuid::new_v4()));
            let pasteboard: Option<Retained<NSPasteboard>> =
                unsafe { msg_send![NSPasteboard::class(), pasteboardWithName: &*name] };
            let Some(pasteboard) = pasteboard else {
                return false;
            };
            let string_type = unsafe { NSPasteboardTypeString };
            let png_type = unsafe { NSPasteboardTypePNG };
            let file_url_type = unsafe { NSPasteboardTypeFileURL };

            let text_item = NSPasteboardItem::new();
            assert!(text_item.setString_forType(&NSString::from_str("hello"), string_type));
            let image_item = NSPasteboardItem::new();
            let mut image_writer = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgba8(2, 2)
                .write_to(&mut image_writer, ImageFormat::Png)
                .expect("encode test image");
            let png = image_writer.into_inner();
            assert!(image_item.setData_forType(&NSData::with_bytes(&png), png_type));
            let first_file = NSPasteboardItem::new();
            let second_file = NSPasteboardItem::new();
            assert!(first_file
                .setString_forType(&NSString::from_str("file:///tmp/one.txt"), file_url_type,));
            assert!(second_file
                .setString_forType(&NSString::from_str("file:///tmp/two.txt"), file_url_type,));
            let objects = vec![
                ProtocolObject::<dyn NSPasteboardWriting>::from_retained(text_item),
                ProtocolObject::<dyn NSPasteboardWriting>::from_retained(image_item),
                ProtocolObject::<dyn NSPasteboardWriting>::from_retained(first_file),
                ProtocolObject::<dyn NSPasteboardWriting>::from_retained(second_file),
            ];
            assert!(pasteboard.writeObjects(&NSArray::from_retained_slice(&objects)));

            let items = pasteboard.pasteboardItems().expect("pasteboard items");
            assert_eq!(
                items
                    .iter()
                    .next()
                    .expect("text item")
                    .stringForType(string_type)
                    .expect("text value")
                    .to_string(),
                "hello"
            );
            assert_eq!(
                items
                    .iter()
                    .nth(1)
                    .expect("image item")
                    .dataForType(png_type)
                    .expect("image data")
                    .to_vec(),
                png
            );
            let files: Vec<String> = items
                .iter()
                .filter_map(|item| item.stringForType(file_url_type))
                .map(|value| value.to_string())
                .collect();
            assert_eq!(files, ["file:///tmp/one.txt", "file:///tmp/two.txt"]);
            pasteboard.clearContents();
            true
        });
        if ran {
            println!("named NSPasteboard integration test passed");
        } else {
            println!("named NSPasteboard unavailable in this non-bundled test process; skipped");
        }
    }
}

#[cfg(target_os = "macos")]
fn main() {
    macos_test::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("NSPasteboard integration test is macOS-only; skipped");
}
