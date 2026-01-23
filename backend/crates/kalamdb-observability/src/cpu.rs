//! CPU detection and monitoring utilities
//!
//! Provides centralized CPU core detection for parallelism configuration
//! across the system.

/// Get the number of logical CPU cores available
///
/// Returns the number of logical CPU cores (including hyper-threading).
/// Always returns at least 1, even if detection fails.
///
/// # Examples
///
/// ```
/// use kalamdb_observability::cpu::get_cpu_count;
///
/// let cpu_count = get_cpu_count();
/// assert!(cpu_count >= 1);
/// ```
#[inline]
pub fn get_cpu_count() -> usize {
    num_cpus::get().max(1)
}

/// Get the number of physical CPU cores available
///
/// Returns the number of physical CPU cores (excluding hyper-threading).
/// Always returns at least 1, even if detection fails.
///
/// # Examples
///
/// ```
/// use kalamdb_observability::cpu::get_physical_cpu_count;
///
/// let physical_count = get_physical_cpu_count();
/// assert!(physical_count >= 1);
/// ```
#[inline]
pub fn get_physical_cpu_count() -> usize {
    num_cpus::get_physical().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cpu_count() {
        let count = get_cpu_count();
        assert!(count >= 1);
    }

    #[test]
    fn test_get_physical_cpu_count() {
        let count = get_physical_cpu_count();
        assert!(count >= 1);
    }

    #[test]
    fn test_logical_ge_physical() {
        let logical = get_cpu_count();
        let physical = get_physical_cpu_count();
        assert!(logical >= physical);
    }
}
