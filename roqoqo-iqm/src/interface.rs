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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use qoqo_calculator::CalculatorFloat;
use roqoqo::operations::*;
use roqoqo::RoqoqoBackendError;

/// Convert a qubit number into the format accepted by IQM.
// e.g. "QB2" for qoqo_qubit number 1 (IQM qubits start from 1)
#[inline]
fn _convert_qubit_name(qoqo_qubit: usize) -> String {
    format!("QB{}", qoqo_qubit + 1)
}

/// Create a vector will all qubit names, in the format accepted by IQM
#[inline]
fn _convert_all_qubit_names(number_qubits: usize) -> Vec<String> {
    let mut qubit_vec = vec![];
    for i in 1..=number_qubits {
        qubit_vec.push(format!("QB{}", i))
    }
    qubit_vec
}

/// Representation for quantum circuits accepted by the IQM REST API.
///
/// roqoqo does not have a `name` identifier for quantum circuits, but it is needed when
/// submitting to the IQM backend.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct IqmCircuit {
    /// Name of the circuit
    pub name: String,
    /// Vector of instructions accepted by the IQM REST API
    pub instructions: Vec<IqmInstruction>,
    // TODO
    // pub metadata : Option<HashMap< String, String >>,
}

/// Representation for instructions accepted by the IQM REST API
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct IqmInstruction {
    /// Identifies the type of instruction, which can be a gate, a measurement or a barrier
    pub name: String,
    /// The qubits involved in the operation
    pub qubits: Vec<String>,
    /// Arguments of the instruction. They depend on the type of operation, and can hold both gate
    ///parameters or measurement names. The latter are used as register names when converting the
    /// results into roqoqo registers.
    pub args: HashMap<String, CalculatorFloat>,
}

/// Converts all operations in a [roqoqo::Circuit] into instructions for IQM Hardware or IQM Simulators
///
/// # Arguments
///
/// `circuit` - The [roqoqo::Circuit] that is converted
///
/// `device_number_qubits` - The number of qubits of the backend device. It is used to know how many
///  qubits to measure with [roqoqo::operations::PragmaRepeatedMeasurement]
///
/// # Returns
///
/// `Ok(IqmCircuit)` - Converted circuit `Err(RoqoqoBackendError::OperationNotInBackend)` - Error
/// when [roqoqo::operations::Operation] can not be converted
pub fn call_circuit<'a>(
    circuit: impl Iterator<Item = &'a Operation>,
    device_number_qubits: usize,
) -> Result<(IqmCircuit, usize), RoqoqoBackendError> {
    let mut circuit_vec: Vec<IqmInstruction> = Vec::new();
    let mut number_measurements: usize = 1;

    for op in circuit {
        match op {
            // Do nothing with definitions of registers, since the name of the output register will
            // be contained in the `args` field of the measurement instruction
            Operation::DefinitionBit(_) => {}
            Operation::DefinitionFloat(_) => {}
            Operation::DefinitionComplex(_) => {}
            Operation::MeasureQubit(o) => {
                let readout = o.readout().clone();
                let mut found: bool = false;
                // Check if we already have a measurement to the same register
                // if yes, add the qubit being measured to that measurement
                for i in &mut circuit_vec {
                    if i.name == "measurement" {
                        let meas_readout =
                            i.args.get("key").ok_or(RoqoqoBackendError::GenericError {
                                msg: "A measurement must contain a `key` entry in the `args` field"
                                    .to_string(),
                            })?;
                        if let CalculatorFloat::Str(s) = meas_readout {
                            if s == &readout {
                                i.qubits.push(_convert_qubit_name(*o.qubit()));
                                found = true;
                                break;
                            }
                        }
                    }
                }
                if !found {
                    // This is executed only if no measurement to the same register was found
                    // In this case, a new IqmInstruction is created corresponding to the measurement
                    let meas = IqmInstruction {
                        name: "measurement".to_string(),
                        qubits: vec![_convert_qubit_name(*o.qubit())],
                        args: HashMap::from([("key".to_string(), CalculatorFloat::Str(readout))]),
                    };
                    circuit_vec.push(meas)
                }
            }
            Operation::PragmaRepeatedMeasurement(o) => {
                number_measurements = *o.number_measurements();
                let readout = o.readout().clone();

                let measure_all = IqmInstruction {
                    name: "measurement".to_string(),
                    qubits: _convert_all_qubit_names(device_number_qubits),
                    args: HashMap::from([("key".to_string(), CalculatorFloat::Str(readout))]),
                };
                circuit_vec.push(measure_all);
            }
            Operation::PragmaLoop(o) => {
                // do some type conversion to get the number of repetitions as usize
                let reps_ref =
                    o.repetitions()
                        .float()
                        .map_err(|_| {
                            RoqoqoBackendError::GenericError {
                        msg:
                            "Only Loops with non-symbolic repetitions are supported by the backend."
                                .to_string(),
                    }
                        })?;
                let reps = (*reps_ref) as i32;
                // add the inner circuit for the specified number of repetitions
                for _ in 0..reps {
                    for i in o.circuit().iter() {
                        let instruction = call_operation(i)?;
                        circuit_vec.push(instruction);
                    }
                }
            }
            _ => {
                let instruction = call_operation(op)?;
                circuit_vec.push(instruction)
            }
        };
    }

    let iqm_circuit = IqmCircuit {
        // TODO
        // circuits have to be given different names when support for circuit batches is added
        // Since for the moment we only support submission of a single circuit, the name is
        // irrelevant and is hardcoded
        name: String::from("my_qc"),
        instructions: circuit_vec,
    };

    Ok((iqm_circuit, number_measurements))
}

/// Converts a [roqoqo::operations::Operation] into a native instruction for IQM Hardware
///
/// # Arguments
///
/// `operation` - The [roqoqo::operations::Operation] that is converted
///
/// # Returns
///
/// `Ok(IqmInstruction)` - Converted instruction  
/// `Err(RoqoqoBackendError::OperationNotInBackend)` - Error when [roqoqo::operations::Operation] can not be converted
pub fn call_operation(operation: &Operation) -> Result<IqmInstruction, RoqoqoBackendError> {
    let mut op_parameters = HashMap::new();

    match operation {
        Operation::RotateXY(op) => {
            op_parameters.insert(
                "angle_t".to_string(),
                CalculatorFloat::Float(*op.theta().float()?),
            );
            op_parameters.insert(
                "phase_t".to_string(),
                CalculatorFloat::Float(*op.phi().float()?),
            );

            Ok(IqmInstruction {
                name: "phased_rx".to_string(),
                qubits: vec![_convert_qubit_name(*op.qubit())],
                args: op_parameters,
            })
        }
        Operation::ControlledPauliZ(op) => {
            let control = _convert_qubit_name(*op.control());
            let target = _convert_qubit_name(*op.target());

            Ok(IqmInstruction {
                name: "cz".to_string(),
                qubits: vec![control, target],
                args: op_parameters,
            })
        }
        _ => Err(RoqoqoBackendError::OperationNotInBackend {
            backend: "IQM",
            hqslang: operation.hqslang(),
        }),
    }
}
