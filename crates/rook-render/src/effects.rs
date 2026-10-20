//! Particles and ribbons simulated on the CPU: the emitters' particles
//! ([`particles`]) and Beams and Trails as strips ([`ribbons`]). The GPU
//! draws them in `renderer::effects`.

pub(crate) mod particles;
pub(crate) mod ribbons;
