//! JDK native-object shims backed by VM heap objects.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use super::descriptors::MethodDescriptor;
use super::frame::Frame;
use super::heap::{FieldKey, ObjectRef};
use super::interpreter::Interpreter;
use super::native;
use super::value::Value;
use crate::{JayError, JayResult};

impl<'a, W: Write> Interpreter<'a, W> {
    pub(super) fn invoke_integer_value_of(
        &mut self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_name: &str,
    ) -> JayResult<()> {
        let arguments = self.pop_method_arguments(
            caller,
            descriptor,
            &format!("invokestatic target {target_name}"),
        )?;
        let [Value::Int(value)] = arguments.as_slice() else {
            return Err(JayError::new("Integer.valueOf expected one int argument"));
        };

        let reference = self.heap.allocate_instance("java/lang/Integer");
        self.heap
            .put_instance_field(reference, integer_value_field(), Value::Int(*value))?;
        caller.stack.push(Value::Reference(reference));
        self.collect_if_needed(caller);
        Ok(())
    }

    pub(super) fn invoke_string_value_of_object(
        &mut self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_name: &str,
    ) -> JayResult<()> {
        let arguments = self.pop_method_arguments(
            caller,
            descriptor,
            &format!("invokestatic target {target_name}"),
        )?;
        let [value] = arguments.as_slice() else {
            return Err(JayError::new(
                "String.valueOf(Object) expected one argument",
            ));
        };

        match value {
            Value::Null => {
                let reference = self.heap.allocate_string("null");
                caller.stack.push(Value::Reference(reference));
                self.collect_if_needed(caller);
            }
            Value::Reference(reference) => {
                let class_name = self.heap.instance_class_name(*reference).ok();
                match class_name.as_deref() {
                    Some("java/lang/Integer") => {
                        let text = self.boxed_integer_value(*reference)?.to_string();
                        let reference = self.heap.allocate_string(text);
                        caller.stack.push(Value::Reference(reference));
                        self.collect_if_needed(caller);
                    }
                    _ if matches!(
                        self.heap.value_type(*reference)?,
                        Some(super::descriptors::ValueType::Reference(ref name))
                            if name == "java/lang/String"
                    ) =>
                    {
                        caller.stack.push(Value::Reference(*reference));
                    }
                    _ => {
                        let text = self.println_object_text(value.clone())?;
                        let reference = self.heap.allocate_string(text);
                        caller.stack.push(Value::Reference(reference));
                        self.collect_if_needed(caller);
                    }
                }
            }
            other => {
                return Err(JayError::new(format!(
                    "String.valueOf(Object) received {}",
                    other.type_name(&self.heap)?
                )));
            }
        }

        Ok(())
    }

    pub(super) fn invoke_pattern_matches(
        &mut self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_name: &str,
    ) -> JayResult<()> {
        let arguments = self.pop_method_arguments(
            caller,
            descriptor,
            &format!("invokestatic target {target_name}"),
        )?;
        let [pattern, input] = arguments.as_slice() else {
            return Err(JayError::new(
                "Pattern.matches expected pattern and CharSequence arguments",
            ));
        };

        let Value::Reference(pattern) = pattern else {
            return Err(JayError::new("Pattern.matches received null pattern"));
        };
        let Value::Reference(input) = input else {
            return Err(JayError::new("Pattern.matches received null input"));
        };

        let pattern = self.heap.string(*pattern)?;
        let input = self
            .heap
            .string(*input)
            .map_err(|_| JayError::new("Pattern.matches currently supports String input only"))?;
        let matched = native::pattern_matches(pattern, input)?;
        caller.stack.push(Value::Int(if matched { 1 } else { 0 }));
        Ok(())
    }

    pub(super) fn boxed_integer_value(&self, reference: ObjectRef) -> JayResult<i32> {
        match self
            .heap
            .get_instance_field(reference, &integer_value_field())?
        {
            Some(Value::Int(value)) => Ok(value),
            None => Err(JayError::new("Integer value has not been initialized")),
            Some(other) => Err(JayError::new(format!(
                "Integer value found {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    pub(super) fn invoke_time_zone_get_time_zone(
        &mut self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_name: &str,
    ) -> JayResult<()> {
        let arguments = self.pop_method_arguments(
            caller,
            descriptor,
            &format!("invokestatic target {target_name}"),
        )?;
        let [id] = arguments.as_slice() else {
            return Err(JayError::new(
                "TimeZone.getTimeZone expected one ID argument",
            ));
        };
        let Value::Reference(id) = id else {
            return Err(JayError::new("TimeZone.getTimeZone received null ID"));
        };

        let requested_id = self.heap.string(*id)?.to_string();
        let time_zone = native::TimeZone::from_id(&requested_id);
        let id_reference = self.heap.allocate_string(time_zone.id());
        let reference = self.heap.allocate_instance("java/util/TimeZone");

        self.heap.put_instance_field(
            reference,
            time_zone_id_field(),
            Value::Reference(id_reference),
        )?;
        self.heap.put_instance_field(
            reference,
            time_zone_offset_field(),
            Value::Long(time_zone.offset_millis()),
        )?;
        caller.stack.push(Value::Reference(reference));
        self.collect_if_needed(caller);
        Ok(())
    }

    pub(super) fn invoke_local_date_time_now(&mut self, caller: &mut Frame) -> JayResult<()> {
        let epoch_millis = current_time_millis()?;
        let reference = self.heap.allocate_instance("java/time/LocalDateTime");
        self.heap.put_instance_field(
            reference,
            local_date_time_epoch_millis_field(),
            Value::Long(epoch_millis),
        )?;
        caller.stack.push(Value::Reference(reference));
        self.collect_if_needed(caller);
        Ok(())
    }

    pub(super) fn invoke_date_to_string(
        &mut self,
        caller: &mut Frame,
        receiver: ObjectRef,
    ) -> JayResult<()> {
        let fast_time = self.date_fast_time(receiver)?;
        let reference = self.heap.allocate_string(native::date_to_string(fast_time));
        caller.stack.push(Value::Reference(reference));
        self.collect_if_needed(caller);
        Ok(())
    }

    pub(super) fn invoke_simple_date_format(
        &mut self,
        caller: &mut Frame,
        receiver: ObjectRef,
        arguments: &[Value],
    ) -> JayResult<()> {
        let [date] = arguments else {
            return Err(JayError::new(
                "SimpleDateFormat.format expected one Date argument",
            ));
        };
        let Value::Reference(date) = date else {
            return Err(JayError::new("SimpleDateFormat.format received null Date"));
        };

        let pattern = self.simple_date_format_pattern(receiver)?;
        let time_zone = self.simple_date_format_time_zone(receiver)?;
        let fast_time = self.date_fast_time(*date)?;
        let output = native::format_simple_date(&pattern, fast_time, time_zone)?;
        let reference = self.heap.allocate_string(output);
        caller.stack.push(Value::Reference(reference));
        self.collect_if_needed(caller);
        Ok(())
    }

    pub(super) fn invoke_simple_date_format_set_time_zone(
        &mut self,
        receiver: ObjectRef,
        arguments: &[Value],
    ) -> JayResult<()> {
        let [time_zone] = arguments else {
            return Err(JayError::new(
                "SimpleDateFormat.setTimeZone expected one TimeZone argument",
            ));
        };
        let Value::Reference(time_zone) = time_zone else {
            return Err(JayError::new(
                "SimpleDateFormat.setTimeZone received null TimeZone",
            ));
        };

        self.heap.put_instance_field(
            receiver,
            simple_date_format_time_zone_field(),
            Value::Reference(*time_zone),
        )
    }

    pub(super) fn invoke_simple_date_format_constructor(
        &mut self,
        caller: &mut Frame,
        descriptor: &MethodDescriptor,
        target_name: &str,
    ) -> JayResult<()> {
        let arguments = self.pop_constructor_arguments(
            caller,
            descriptor,
            &format!("invokespecial constructor target {target_name}"),
        )?;
        let [pattern] = arguments.as_slice() else {
            return Err(JayError::new(
                "SimpleDateFormat constructor expected one pattern argument",
            ));
        };
        let Value::Reference(pattern) = pattern else {
            return Err(JayError::new(
                "SimpleDateFormat constructor received null pattern",
            ));
        };
        let receiver = caller.pop_object_ref()?;
        let field = FieldKey::new(
            "java/text/SimpleDateFormat",
            "pattern",
            "Ljava/lang/String;",
        );
        self.heap
            .put_instance_field(receiver, field, Value::Reference(*pattern))
    }

    pub(super) fn date_fast_time(&self, date: ObjectRef) -> JayResult<i64> {
        let field = FieldKey::new("java/util/Date", "fastTime", "J");
        match self.heap.get_instance_field(date, &field)? {
            Some(Value::Long(value)) => Ok(value),
            None => Ok(0),
            Some(other) => Err(JayError::new(format!(
                "java.util.Date.fastTime found {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    pub(super) fn local_date_time_epoch_millis(
        &self,
        local_date_time: ObjectRef,
    ) -> JayResult<i64> {
        match self
            .heap
            .get_instance_field(local_date_time, &local_date_time_epoch_millis_field())?
        {
            Some(Value::Long(value)) => Ok(value),
            None => Err(JayError::new(
                "LocalDateTime epoch millis has not been initialized",
            )),
            Some(other) => Err(JayError::new(format!(
                "LocalDateTime epoch millis found {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    pub(super) fn simple_date_format_pattern(&self, formatter: ObjectRef) -> JayResult<String> {
        let field = FieldKey::new(
            "java/text/SimpleDateFormat",
            "pattern",
            "Ljava/lang/String;",
        );
        match self.heap.get_instance_field(formatter, &field)? {
            Some(Value::Reference(reference)) => Ok(self.heap.string(reference)?.to_string()),
            Some(Value::Null) | None => Err(JayError::new(
                "SimpleDateFormat pattern has not been initialized",
            )),
            Some(other) => Err(JayError::new(format!(
                "SimpleDateFormat pattern found {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    pub(super) fn simple_date_format_time_zone(
        &self,
        formatter: ObjectRef,
    ) -> JayResult<native::TimeZone> {
        match self
            .heap
            .get_instance_field(formatter, &simple_date_format_time_zone_field())?
        {
            Some(Value::Reference(reference)) => self.time_zone(reference),
            Some(Value::Null) | None => Ok(native::TimeZone::gmt()),
            Some(other) => Err(JayError::new(format!(
                "SimpleDateFormat timeZone found {}",
                other.type_name(&self.heap)?
            ))),
        }
    }

    pub(super) fn time_zone(&self, reference: ObjectRef) -> JayResult<native::TimeZone> {
        let id = match self
            .heap
            .get_instance_field(reference, &time_zone_id_field())?
        {
            Some(Value::Reference(id)) => self.heap.string(id)?.to_string(),
            Some(Value::Null) | None => {
                return Err(JayError::new("TimeZone ID has not been initialized"));
            }
            Some(other) => {
                return Err(JayError::new(format!(
                    "TimeZone ID found {}",
                    other.type_name(&self.heap)?
                )));
            }
        };

        let offset_millis = match self
            .heap
            .get_instance_field(reference, &time_zone_offset_field())?
        {
            Some(Value::Long(value)) => value,
            None => return Err(JayError::new("TimeZone offset has not been initialized")),
            Some(other) => {
                return Err(JayError::new(format!(
                    "TimeZone offset found {}",
                    other.type_name(&self.heap)?
                )));
            }
        };

        Ok(native::TimeZone::resolved(id, offset_millis))
    }
}

fn simple_date_format_time_zone_field() -> FieldKey {
    FieldKey::new(
        "java/text/SimpleDateFormat",
        "__jay_timeZone",
        "Ljava/util/TimeZone;",
    )
}

fn time_zone_id_field() -> FieldKey {
    FieldKey::new("java/util/TimeZone", "__jay_id", "Ljava/lang/String;")
}

fn time_zone_offset_field() -> FieldKey {
    FieldKey::new("java/util/TimeZone", "__jay_offsetMillis", "J")
}

fn local_date_time_epoch_millis_field() -> FieldKey {
    FieldKey::new("java/time/LocalDateTime", "__jay_epochMillis", "J")
}

fn integer_value_field() -> FieldKey {
    FieldKey::new("java/lang/Integer", "value", "I")
}

pub(super) fn current_time_millis() -> JayResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| JayError::new(format!("system time is before Unix epoch: {error}")))?;
    i64::try_from(duration.as_millis())
        .map_err(|_| JayError::new("current time milliseconds exceed long range"))
}
