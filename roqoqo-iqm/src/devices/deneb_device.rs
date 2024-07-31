// Copyright © 2020-2023 HQS Quantum Simulations GmbH. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

use ndarray::Array2;
use roqoqo::devices::{Device, GenericDevice};
use roqoqo::operations::{Operation, SingleQubitOperation};
use roqoqo::prelude::*;
use roqoqo::{Circuit, RoqoqoBackendError};

use crate::IqmBackendError;

/// IQM Deneb device
///
/// A hardware device composed of six qubits each coupled to a central resonator.
#[derive(Debug, PartialEq, Eq, Clone, serde::Serialize, serde::Deserialize)]
pub struct DenebDevice {
    url: String,
    name: String,
}

impl DenebDevice {
    /// Create new DenebDevice with default settings.
    pub fn new() -> Self {
        Self {
            url: "https://cocos.resonance.meetiqm.com/deneb/jobs".to_string(),
            name: "Deneb".to_string(),
        }
    }

    /// Returns API endpoint URL of the device.
    pub fn remote_host(&self) -> String {
        self.url.clone()
    }

    /// Returns the name of the device.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Change API endpoint URL of the device
    ///
    /// # Arguments
    ///
    /// * `new_url` - The new URL to set.
    pub fn set_endpoint_url(&mut self, new_url: String) {
        self.url = new_url
    }

    /// Validate the circuit to be run for Deneb's architecture.
    ///
    /// This involves checking
    /// 1) The device's connectivity
    /// 2) The presence of subsequent Load operations or subsequent Store operations, which are not
    ///    allowed since only a single excitation can be stored in the resonator at any time.
    ///
    /// # Arguments
    ///
    /// * `circuit` - The circuit to be validated.
    ///
    /// # Returns
    ///
    /// * `Err(RoqoqoBackendError)` - The circuit is invalid.
    pub fn validate_circuit(&self, circuit: &Circuit) -> Result<(), IqmBackendError> {
        self.validate_circuit_connectivity(circuit)?;
        self.validate_circuit_load_store(circuit)?;
        Ok(())
    }

    /// Validate the circuit Load/Store combinations for Deneb's architecture.
    ///
    /// Invalid combinations are:
    /// 1) Multiple subsequent SingleExcitatoinLoad.
    /// 2) Multiple subsequent SingleExcitatoinStore.
    /// 3) A combination like Store - RotateXY - Load where the qubit involved in all three
    ///    operations is the same.
    ///
    /// # Arguments
    ///
    /// * `circuit` - The circuit to be validated.
    ///
    /// # Returns
    ///
    /// * `Err(RoqoqoBackendError)` - The circuit is invalid.
    fn validate_circuit_load_store(&self, circuit: &Circuit) -> Result<(), IqmBackendError> {
        enum State {
            FoundLoad,
            FoundStore,
            Zero,
        }

        let mut state = State::Zero;
        let mut stored_qubit: Option<usize> = None;
        let mut qubit_rotated = false;

        for op in circuit.iter() {
            match op {
                Operation::SingleExcitationLoad(o) => {
                    match state {
                        State::FoundStore => {
                            if qubit_rotated {
                                let loaded_qubit = o.qubit();
                                if let Some(stored) = stored_qubit {
                                    if *loaded_qubit == stored {
                                        return Err(IqmBackendError::InvalidCircuit {
                                            msg: format!(
                                                "Circuit tries to rotate qubit {} before loading an \
                                                 excitation into it from the resonator.",
                                                loaded_qubit)
                                        });
                                    }
                                }
                            }
                        }
                        State::FoundLoad => {
                            return Err(IqmBackendError::InvalidCircuit {
                                msg: "Circuit tries to load twice in a row from the resonator"
                                    .to_string(),
                            })
                        }
                        _ => {}
                    }
                    state = State::FoundLoad;
                }
                Operation::SingleExcitationStore(o) => {
                    if let State::FoundStore = state {
                        return Err(IqmBackendError::InvalidCircuit {
                            msg: "Circuit tries to store two excitations in the resonator."
                                .to_string(),
                        });
                    }
                    stored_qubit = Some(*o.qubit());
                    state = State::FoundStore;
                }
                _ => {
                    if let Some(stored) = stored_qubit {
                        if let Ok(inner_op) = SingleQubitOperation::try_from(op) {
                            let qubit = inner_op.qubit();
                            if stored == *qubit {
                                qubit_rotated = true;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate the circuit's connectivity for Deneb's architecture.
    ///
    /// # Arguments
    ///
    /// * `circuit` - The circuit to be validated.
    ///
    /// # Returns
    ///
    /// * `Err(RoqoqoBackendError)` - The circuit is invalid.
    fn validate_circuit_connectivity(&self, circuit: &Circuit) -> Result<(), IqmBackendError> {
        let allowed_measurement_ops = [
            "PragmaSetNumberOfMeasurements",
            "PragmaRepeatedMeasurement",
            "MeasureQubit",
            "DefinitionBit",
            "InputBit",
        ];

        for op in circuit.iter() {
            match op {
                Operation::RotateXY(o) => {
                    let qubit = *o.qubit();
                    if qubit >= self.number_qubits() {
                        return Err(IqmBackendError::InvalidCircuit {
                            msg: format!(
                                "Too many qubits involved in the circuit: 
                                    Found {} acting on qubit: {} 
                                    Qubits in Deneb device: {}",
                                op.hqslang(),
                                qubit,
                                self.number_qubits()
                            ),
                        });
                    }
                }
                Operation::CZQubitResonator(o) => {
                    let qubit = *o.qubit();
                    let resonator = *o.mode();
                    if qubit >= self.number_qubits() {
                        return Err(IqmBackendError::InvalidCircuit {
                            msg: format!(
                                "Too many qubits involved in the circuit: 
                                    Found {} acting on qubit: {} 
                                    Qubits in Deneb device: {}",
                                op.hqslang(),
                                qubit,
                                self.number_qubits()
                            ),
                        });
                    }
                    if resonator != 0 {
                        return Err(IqmBackendError::InvalidCircuit {
                            msg: format!(
                                "Wrong resonator index in operation {}. DenebDevice has a single \
                                resonator with index 0.",
                                op.hqslang()
                            ),
                        });
                    }
                }
                Operation::SingleExcitationLoad(o) => {
                    let resonator = *o.mode();
                    if resonator != 0 {
                        return Err(IqmBackendError::InvalidCircuit {
                            msg: format!(
                                "Wrong resonator index in operation {}. DenebDevice has a single \
                                resonator with index 0.",
                                op.hqslang()
                            ),
                        });
                    }
                }
                Operation::SingleExcitationStore(o) => {
                    let resonator = *o.mode();
                    if resonator != 0 {
                        return Err(IqmBackendError::InvalidCircuit {
                            msg: format!(
                                "Wrong resonator index in operation {}. DenebDevice has a single \
                                resonator with index 0.",
                                op.hqslang()
                            ),
                        });
                    }
                }
                _ => {
                    if !allowed_measurement_ops.contains(&op.hqslang()) {
                        return Err(IqmBackendError::RoqoqoBackendError(
                            RoqoqoBackendError::OperationNotInBackend {
                                backend: "IQM",
                                hqslang: op.hqslang(),
                            },
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Implements the Device trait for DenebDevice.
///
/// Defines standard functions available for roqoqo-iqm devices.
impl Device for DenebDevice {
    /// Returns the gate time of a single qubit operation if the single qubit operation is available
    /// on device.
    ///
    /// # Arguments
    ///
    /// * `hqslang` - The hqslang name of a single qubit gate.
    /// * `qubit` - The qubit the gate acts on
    ///
    /// # Returns
    ///
    /// * `Some<f64>` - The gate time.
    /// * `None` - The gate is not available on the device.
    fn single_qubit_gate_time(&self, hqslang: &str, qubit: &usize) -> Option<f64> {
        if hqslang == "RotateXY" && qubit < &self.number_qubits() {
            Some(1.0)
        } else {
            None
        }
    }

    // NOTE
    // Since only a two-dimensional subspace of the resonator is accessible, it effectively behaves
    // like a qubit, and the resonator gates are treated here as two-qubit gates even if, according
    // to their definition in roqoqo, they are at the same time single-qubit gates and single-mode gates.
    /// Returns the gate time of a qubit-resonator operation if the operation is available on device.
    ///
    /// Note that in this method the control qubit is the actual qubit involved, while the target
    /// qubit corresponds to the central resonator of the Deneb device.
    ///
    /// # Arguments
    ///
    /// * `hqslang` - The hqslang name of a two qubit gate.
    /// * `control` - The control qubit the gate acts on
    /// * `target` - The target qubit the gate acts on
    ///
    /// # Returns
    ///
    /// * `Some<f64>` - The gate time.
    /// * `None` - The gate is not available on the device.
    fn two_qubit_gate_time(&self, hqslang: &str, control: &usize, target: &usize) -> Option<f64> {
        if target == &0_usize {
            match hqslang {
                "CZQubitResonator" => {
                    if control < &self.number_qubits() {
                        Some(1.0)
                    } else {
                        None
                    }
                }
                "SingleExcitationLoad" => {
                    if control < &self.number_qubits() {
                        Some(1.0)
                    } else {
                        None
                    }
                }
                "SingleExcitationStore" => {
                    if control < &self.number_qubits() {
                        Some(1.0)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        }
    }

    /// Returns the gate time of a three qubit operation if the three qubit operation is available on device.
    ///
    /// # Arguments
    ///
    /// * `hqslang` - The hqslang name of a three qubit gate.
    /// * `control` - The control qubit the gate acts on
    /// * `target` - The target qubit the gate acts on
    ///
    /// # Returns
    ///
    /// * `Some<f64>` - The gate time.
    /// * `None` - The gate is not available on the device.
    fn three_qubit_gate_time(
        &self,
        _hqslang: &str,
        _control_0: &usize,
        _control_1: &usize,
        _target: &usize,
    ) -> Option<f64> {
        None
    }

    /// Returns the gate time of a multi qubit operation if the multi qubit operation is available on device.
    ///
    /// # Arguments
    ///
    /// * `hqslang` - The hqslang name of a multi qubit gate.
    /// * `qubits` - The qubits the gate acts on
    ///
    /// # Returns
    ///
    /// * `Some<f64>` - The gate time.
    /// * `None` - The gate is not available on the device.
    fn multi_qubit_gate_time(&self, _hqslang: &str, _qubits: &[usize]) -> Option<f64> {
        None
    }

    /// Returns the matrix of the decoherence rates of the Lindblad equation.
    ///
    /// $$
    /// \frac{d}{dt}\rho = \sum_{i,j=0}^{2} M_{i,j} L_{i} \rho L_{j}^{\dagger} - \frac{1}{2} \{ L_{j}^{\dagger} L_i, \rho \} \\\\
    ///     L_0 = \sigma^{+} \\\\
    ///     L_1 = \sigma^{-} \\\\
    ///     L_3 = \sigma^{z}
    /// $$
    ///
    /// # Arguments
    ///
    /// * `qubit` - The qubit for which the rate matrix is returned.
    ///
    /// # Returns
    ///
    /// * `Some<Array2<f64>>` - The decoherence rates.
    /// * `None` - The qubit is not part of the device.
    fn qubit_decoherence_rates(&self, _qubit: &usize) -> Option<Array2<f64>> {
        None
    }

    /// Returns the number of qubits the device supports.
    ///
    /// # Returns
    ///
    /// The number of qubits in the device.
    fn number_qubits(&self) -> usize {
        6
    }

    /// Returns the list of pairs of qubits linked with a native two-qubit-gate in the device.
    ///
    /// A pair of qubits is considered linked by a native two-qubit-gate if the device
    /// can implement a two-qubit-gate between the two qubits without decomposing it
    /// into a sequence of gates that involves a third qubit of the device.
    /// The two-qubit-gate also has to form a universal set together with the available
    /// single qubit gates.
    ///
    /// The returned vectors is a simple, graph-library independent, representation of the
    /// undirected connectivity graph of the device. It can be used to construct the connectivity
    /// graph in a graph library of the users choice from a list of edges and can be used for
    /// applications like routing in quantum algorithms.
    ///
    /// # Returns
    ///
    /// * `Vec<(usize, usize)>` - A list (Vec) of pairs of qubits linked with a native two-qubit-gate in the device.
    fn two_qubit_edges(&self) -> Vec<(usize, usize)> {
        vec![]
    }

    /// Turns Device into GenericDevice.
    ///
    /// Can be used as a generic interface for devices when a boxed dyn trait object cannot be used
    /// (for example when the interface needs to be serialized)
    ///
    /// # Note
    ///
    /// [crate::devices::GenericDevice] uses nested HashMaps to represent the most general device
    /// connectivity. The memory usage will be inefficient for devices with large qubit numbers.
    ///
    /// # Returns
    ///
    /// * `GenericDevice` - A generic device representation of the device.
    fn to_generic_device(&self) -> GenericDevice {
        let mut generic_device = GenericDevice::new(self.number_qubits());
        for qubit in 0..self.number_qubits() {
            generic_device
                .set_single_qubit_gate_time(
                    "RotateXY",
                    qubit,
                    self.single_qubit_gate_time("RotateXY", &qubit).unwrap(),
                )
                .expect("Unexpectedly failed to add single qubit gate time to generic device.");
            generic_device
                .set_single_qubit_gate_time(
                    "SingleExcitationLoad",
                    qubit,
                    self.single_qubit_gate_time("SingleExcitationLoad", &qubit)
                        .unwrap(),
                )
                .expect("Unexpectedly failed to add single qubit gate time to generic device.");
            generic_device
                .set_single_qubit_gate_time(
                    "SingleExcitationStore",
                    qubit,
                    self.single_qubit_gate_time("SingleExcitationStore", &qubit)
                        .unwrap(),
                )
                .expect("Unexpectedly failed to add single qubit gate time to generic device.");
            generic_device
                .set_single_qubit_gate_time(
                    "CZQubitResonator",
                    qubit,
                    self.single_qubit_gate_time("CZQubitResonator", &qubit)
                        .unwrap(),
                )
                .expect("Unexpectedly failed to add single qubit gate time to generic device.")
        }
        generic_device
    }
}

impl Default for DenebDevice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roqoqo::operations::{
        CZQubitResonator, DefinitionBit, InputBit, RotateXY, SingleExcitationLoad,
        SingleExcitationStore,
    };

    #[test]
    fn test_validate_circuit_passes() {
        let device = DenebDevice::new();
        let mut circuit = Circuit::new();

        circuit += DefinitionBit::new("ro".to_string(), 6, true);
        circuit += InputBit::new("ro".to_string(), 2, true);
        circuit += RotateXY::new(5, 0.5.into(), 0.2.into());
        circuit += RotateXY::new(3, 0.5.into(), 0.2.into());
        circuit += SingleExcitationStore::new(5, 0);
        circuit += CZQubitResonator::new(3, 0);
        circuit += SingleExcitationLoad::new(5, 0);
        circuit += RotateXY::new(5, 0.5.into(), 0.2.into());

        let ok = device.validate_circuit(&circuit);
        assert!(ok.is_ok());
    }

    #[test]
    fn test_validate_circuit_multiple_store() {
        let device = DenebDevice::new();
        let mut circuit = Circuit::new();

        circuit += RotateXY::new(5, 0.5.into(), 0.2.into());
        circuit += SingleExcitationStore::new(5, 0);
        circuit += RotateXY::new(3, 0.5.into(), 0.3.into());
        circuit += SingleExcitationStore::new(5, 0);
        circuit += RotateXY::new(3, 0.5.into(), 0.3.into());

        let err = device.validate_circuit(&circuit);
        assert!(err.is_err());
    }

    #[test]
    fn test_validate_circuit_multiple_load() {
        let device = DenebDevice::new();
        let mut circuit = Circuit::new();

        circuit += RotateXY::new(5, 0.5.into(), 0.5.into());
        circuit += SingleExcitationStore::new(5, 0);
        circuit += RotateXY::new(3, 0.5.into(), 0.3.into());
        circuit += SingleExcitationLoad::new(5, 0);
        circuit += RotateXY::new(3, 0.5.into(), 0.3.into());
        circuit += SingleExcitationLoad::new(5, 0);

        let err = device.validate_circuit(&circuit);
        assert!(err.is_err());
    }

    #[test]
    fn test_validate_circuit_invalid_rotation() {
        let device = DenebDevice::new();
        let mut circuit = Circuit::new();

        circuit += RotateXY::new(5, 0.5.into(), 0.5.into());
        circuit += SingleExcitationStore::new(5, 0);
        circuit += RotateXY::new(5, 0.5.into(), 0.3.into());
        circuit += SingleExcitationLoad::new(5, 0);

        let err = device.validate_circuit(&circuit);
        assert!(err.is_err());
    }
}
