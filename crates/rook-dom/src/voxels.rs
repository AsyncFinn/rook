//! Terrain's voxels. A Terrain holds its voxels decoded, as Roblox's
//! Terrain holds its voxel storage: `SmoothGrid` is their saved form,
//! decoded when it is written (a file loading, a whole write) and encoded
//! when it is read whole (saving, a joining client's copy). An edit changes
//! only the voxels it reaches and journals them as [`Change::Voxels`], so
//! physics, replication and the renderer each take just that region
//! (`docs/behavior/terrain/api.md`). Grids share their chunks' voxels
//! until one writes to them, so taking a region copies no voxels.

use rook_terrain::Grid;
use rook_terrain::grid::edit::Bounds;

use super::*;

impl Dom {
    /// Terrain `id`'s voxels; `None` for an instance holding none.
    pub fn voxels(&self, id: InstanceId) -> Option<&Grid> {
        self.check_not_freed(id);
        self.voxels.get(&id)
    }

    /// Make Terrain `id`'s chunks holding the voxels in `bounds` what
    /// `from`'s are (its own voxels, edited elsewhere, or a region
    /// replicated from the server), and journal the change.
    pub fn write_voxels(
        &mut self,
        id: InstanceId,
        from: &Grid,
        bounds: Bounds,
    ) -> Result<(), DomError> {
        self.inst(id)?;
        self.voxels.entry(id).or_default().put_region(from, bounds);
        self.journal.push(Change::Voxels { id, bounds });
        Ok(())
    }

    /// Edit Terrain `id`'s voxels: `edit` returns the voxels it may have
    /// changed, which are journaled.
    pub fn edit_voxels<R>(
        &mut self,
        id: InstanceId,
        edit: impl FnOnce(&mut Grid) -> (R, Option<Bounds>),
    ) -> Result<R, DomError> {
        self.inst(id)?;
        let (out, changed) = edit(self.voxels.entry(id).or_default());
        if let Some(bounds) = changed {
            self.journal.push(Change::Voxels { id, bounds });
        }
        Ok(out)
    }

    /// Terrain `id`'s `SmoothGrid` as it would be saved, when it holds
    /// voxels: a property its instance doesn't store, for those that copy
    /// every saved property (saving, replication's first copy).
    pub fn voxel_property(&self, id: InstanceId) -> Option<(PropertyId, Value)> {
        Some((self.smooth_grid?, self.encoded_voxels(id)?))
    }

    pub(crate) fn encoded_voxels(&self, id: InstanceId) -> Option<Value> {
        let grid = self.voxels.get(&id)?;
        Some(Value::BinaryString(grid.encode().into()))
    }

    /// Take a whole `SmoothGrid` as `id`'s voxels. Returns false, storing
    /// nothing, when it doesn't decode: the bytes are kept as they are, so
    /// they still save.
    pub(crate) fn store_voxels(&mut self, id: InstanceId, value: &Value) -> Result<bool, DomError> {
        let bytes: &[u8] = match value {
            Value::BinaryString(b) => b.as_ref(),
            _ => return Ok(false),
        };
        match Grid::decode(bytes) {
            Ok(grid) => {
                let property = self.smooth_grid.expect("a Terrain's SmoothGrid");
                self.inst_mut(id)?.properties.remove(&property);
                self.voxels.insert(id, grid);
                Ok(true)
            }
            Err(e) => {
                eprintln!("warning: Terrain.SmoothGrid doesn't decode: {e}");
                self.voxels.remove(&id);
                Ok(false)
            }
        }
    }
}
