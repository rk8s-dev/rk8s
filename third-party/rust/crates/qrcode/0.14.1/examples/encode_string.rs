use qrcode::QrCode;

fn main() {
    let code = QrCode::new(b"Hello").unwrap();
    let string = code.render::<char>().quiet_zone(false).module_dimensions(2, 1).build();
    println!("{}", string);
}
