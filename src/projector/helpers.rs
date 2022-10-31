use super::pack::HeaderPack;

impl HeaderPack {
    pub fn reset_request() {
        let mut full_request = Vec::new();

        for _ in 0..51 {
            full_request.push(HeaderPack {
                ..Default::default()
            });
        }
    }

    pub fn home_request() {
        let mut full_request = Vec::new();

        for _ in 0..51 {
            full_request.push(HeaderPack {
                projector_id: 15.into(),
                home: true,
                enable: true,
                ..Default::default()
            });

            // TODO: add 50 blank frames
        }
    }
}
