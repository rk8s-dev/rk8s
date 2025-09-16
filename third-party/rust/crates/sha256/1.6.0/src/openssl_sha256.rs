use crate::CalculatorSelector;

pub type OpenSslSha256 = openssl::sha::Sha256;

impl CalculatorSelector for OpenSslSha256 {
    type FinishType = [u8; 32];

    fn update_inner(&mut self, data: &[u8]) {
        self.update(data)
    }

    fn finish_inner(self) -> Self::FinishType {
        self.finish()
    }
}
