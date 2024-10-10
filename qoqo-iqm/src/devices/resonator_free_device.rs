// Copyright © 2020-2023 HQS Quantum Simulations GmbH. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the
// License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either
// express or implied. See the License for the specific language governing permissions and
// limitations under the License.

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyByteArray;

use bincode::{deserialize, serialize};
use roqoqo::devices::Device;
use roqoqo_iqm::devices::ResonatorFreeDevice;

/// Six-qubit device similar to the Deneb device, but without the central resonator and with CZ
/// gates available between each pair of qubits. Used to transpile algorithms for use on the Deneb
/// device.
#[pyclass(name = "ResonatorFreeDevice", module = "qoqo_iqm")]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ResonatorFreeDeviceWrapper {
    /// Internal storage of [roqoqo_iqm::ResonatorFreeDevice]
    pub internal: ResonatorFreeDevice,
}

impl ResonatorFreeDeviceWrapper {
    /// Extracts a `ResonatorFreeDevice` from a `ResonatorFreeDeviceWrapper` python object.
    ///
    /// When working with qoqo and other rust based python packages compiled separately a downcast
    /// will not detect that two ResonatorFreeDeviceWrapper objects are compatible. This function tries to
    /// convert a Python object into a ResonatorFreeDevice instance by first checking if the object is a
    /// ResonatorFreeDeviceWrapper instance and, if not, by invoking the to_bincode method on the object and
    /// deserializing the returned binary data.
    ///
    /// Args:
    ///     input (ResonatorFreeDevice): The Python object that should be casted to a [roqoqo_iqm::ResonatorFreeDevice]
    ///
    /// Returns:
    ///     ResonatorFreeDevice: The resulting ResonatorFreeDevice
    ///
    /// Raises:
    ///     PyTypeError: Something went wrong during the downcasting.
    pub fn from_pyany(input: &Bound<PyAny>) -> PyResult<ResonatorFreeDevice> {
        if let Ok(try_downcast) = input.extract::<ResonatorFreeDeviceWrapper>() {
            Ok(try_downcast.internal)
        } else {
            Err(PyTypeError::new_err(
                "Python object cannot be converted to IQM ResonatorFreeDevice: Cast to binary \
                     representation failed"
                    .to_string(),
            ))
        }
    }
}

#[pymethods]
impl ResonatorFreeDeviceWrapper {
    /// Create new simulator device.
    #[new]
    pub fn new() -> Self {
        Self {
            internal: ResonatorFreeDevice::new(),
        }
    }

    /// Return a copy of the ResonatorFreeDevice (copy here produces a deepcopy).
    ///
    /// Returns:
    ///     ResonatorFreeDevice: A deep copy of self.
    pub fn __copy__(&self) -> ResonatorFreeDeviceWrapper {
        self.clone()
    }

    /// Return a deep copy of the ResonatorFreeDevice.
    ///
    /// Returns:
    ///     ResonatorFreeDevice: A deep copy of self.
    pub fn __deepcopy__(&self, _memodict: &Bound<PyAny>) -> ResonatorFreeDeviceWrapper {
        self.clone()
    }

    /// Return the bincode representation of the ResonatorFreeDevice using the [bincode] crate.
    ///
    /// Returns:
    ///     ByteArray: The serialized ResonatorFreeDevice (in [bincode] form).
    ///
    /// Raises:
    ///     ValueError: Cannot serialize ResonatorFreeDevice to bytes.
    pub fn to_bincode(&self) -> PyResult<Py<PyByteArray>> {
        let serialized = serialize(&self.internal)
            .map_err(|_| PyValueError::new_err("Cannot serialize ResonatorFreeDevice to bytes"))?;
        let b: Py<PyByteArray> = Python::with_gil(|py| -> Py<PyByteArray> {
            PyByteArray::new_bound(py, &serialized[..]).into()
        });
        Ok(b)
    }

    /// Convert the bincode representation of the ResonatorFreeDevice to a ResonatorFreeDevice using
    /// the [bincode] crate.
    ///
    /// Args:
    ///     input (ByteArray): The serialized ResonatorFreeDevice (in [bincode] form).
    ///
    /// Returns:
    ///     ResonatorFreeDevice: The deserialized ResonatorFreeDevice.
    ///
    /// Raises:
    ///     TypeError: Input cannot be converted to byte array.
    ///     ValueError: Input cannot be deserialized to ResonatorFreeDevice.
    #[staticmethod]
    pub fn from_bincode(input: &Bound<PyAny>) -> PyResult<ResonatorFreeDeviceWrapper> {
        let bytes = input
            .extract::<Vec<u8>>()
            .map_err(|_| PyTypeError::new_err("Input cannot be converted to byte array"))?;

        Ok(ResonatorFreeDeviceWrapper {
            internal: deserialize(&bytes[..]).map_err(|_| {
                PyValueError::new_err("Input cannot be deserialized to ResonatorFreeDevice")
            })?,
        })
    }

    /// Return number of qubits simulated by ResonatorFreeDevice.
    ///
    /// Returns:
    ///     int: The number of qubits.
    pub fn number_qubits(&self) -> usize {
        self.internal.number_qubits()
    }

    /// Return the list of pairs of qubits linked by a native two-qubit-gate in the device.
    ///
    /// A pair of qubits is considered linked by a native two-qubit-gate if the device
    /// can implement a two-qubit-gate between the two qubits without decomposing it
    /// into a sequence of gates that involves a third qubit of the device.
    /// The two-qubit-gate also has to form a universal set together with the available
    /// single qubit gates.
    ///
    /// The returned vectors is a simple, graph-library independent, representation of the
    /// undirected connectivity graph of the device. It can be used to construct the connectivity
    /// graph in a graph library of the user's choice from a list of edges and can be used for
    /// applications like routing in quantum algorithms.
    ///
    /// Returns:
    ///     list[tuple[int, int]]: The list of two qubit edges.
    fn two_qubit_edges(&self) -> Vec<(usize, usize)> {
        self.internal.two_qubit_edges()
    }

    /// Returns the gate time of a single qubit operation on this device.
    ///
    /// Args:
    ///     hqslang (str): The name of the operation in hqslang format.
    ///     qubit (int): The qubit on which the operation is performed.
    ///
    /// Returns:
    ///     f64: The gate time.
    ///
    /// Raises:
    ///     ValueError: The gate is not available in the device.
    pub fn single_qubit_gate_time(&self, hqslang: &str, qubit: usize) -> PyResult<f64> {
        self.internal
            .single_qubit_gate_time(hqslang, &qubit)
            .ok_or_else(|| PyValueError::new_err("The gate is not available on the device."))
    }

    /// Returns the gate time of a two qubit operation on this device.
    ///
    /// Args:
    ///     hqslang (str): The name of the operation in hqslang format.
    ///     control (int): The control qubit on which the operation is performed.
    ///     target (int): The target qubit on which the operation is performed.
    ///
    /// Returns:
    ///     f64: The gate time.
    ///
    /// Raises:
    ///     ValueError: The gate is not available in the device.
    pub fn two_qubit_gate_time(
        &self,
        hqslang: &str,
        control: usize,
        target: usize,
    ) -> PyResult<f64> {
        self.internal
            .two_qubit_gate_time(hqslang, &control, &target)
            .ok_or_else(|| PyValueError::new_err("The gate is not available on the device."))
    }

    /// Returns the gate time of a multi qubit operation on this device.
    ///
    /// Args:
    ///     hqslang (str): The name of the operation in hqslang format.
    ///     qubits (list[int]): The qubits on which the operation is performed.
    ///
    /// Returns:
    ///     f64: The gate time.
    ///
    /// Raises:
    ///     ValueError: The gate is not available in the device.
    pub fn multi_qubit_gate_time(&self, hqslang: &str, qubits: Vec<usize>) -> PyResult<f64> {
        self.internal
            .multi_qubit_gate_time(hqslang, &qubits)
            .ok_or_else(|| PyValueError::new_err("The gate is not available on the device."))
    }
}
