use crate::pipeline::aggregation::aggregator::AggregationResult;
use crate::pipeline::errors::PipelineError;
use crate::{deserialize, deserialize_i64};
use dozer_core::storage::prefix_transaction::PrefixTransaction;
use dozer_types::types::Field::Int;
use dozer_types::types::{Field, FieldType};

pub struct CountAggregator {}

impl CountAggregator {
    const _AGGREGATOR_ID: u32 = 0x02;

    pub(crate) fn _get_type() -> u32 {
        CountAggregator::_AGGREGATOR_ID
    }

    pub(crate) fn insert(
        cur_state: Option<&[u8]>,
        _new: &Field,
        _return_type: FieldType,
        _txn: &mut PrefixTransaction,
    ) -> Result<AggregationResult, PipelineError> {
        let prev = deserialize_i64!(cur_state);
        let buf = (prev + 1).to_be_bytes();
        Ok(AggregationResult::new(
            Self::get_value(&buf),
            Some(Vec::from(buf)),
        ))
    }

    pub(crate) fn update(
        cur_state: Option<&[u8]>,
        _old: &Field,
        _new: &Field,
        _return_type: FieldType,
        _txn: &mut PrefixTransaction,
    ) -> Result<AggregationResult, PipelineError> {
        let prev = deserialize_i64!(cur_state);
        let buf = (prev).to_be_bytes();
        Ok(AggregationResult::new(
            Self::get_value(&buf),
            Some(Vec::from(buf)),
        ))
    }

    pub(crate) fn delete(
        cur_state: Option<&[u8]>,
        _old: &Field,
        _return_type: FieldType,
        _txn: &mut PrefixTransaction,
    ) -> Result<AggregationResult, PipelineError> {
        let prev = deserialize_i64!(cur_state);
        let buf = (prev - 1).to_be_bytes();
        Ok(AggregationResult::new(
            Self::get_value(&buf),
            Some(Vec::from(buf)),
        ))
    }

    pub(crate) fn get_value(f: &[u8]) -> Field {
        Int(i64::from_be_bytes(deserialize!(f)))
    }
}
