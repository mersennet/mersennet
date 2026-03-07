use revm::primitives::{Address, B256};
use metrics;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundStage {
    Prevote,
    Precommit,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub from: Address,
    pub round: u64,
    pub stage: RoundStage,
    pub block_hash: B256,
    pub height: u64,
}

#[derive(Default, Debug)]
pub struct NetworkSim {
    messages: Vec<Message>,
}

impl NetworkSim {
    pub fn broadcast(&mut self, message: Message) {
        self.messages.push(message);
        metrics::increment_counter!("network_messages_broadcast_total");
        metrics::gauge!("network_messages_pending_gauge", self.messages.len() as f64);
    }

    pub fn drain_round(
        &mut self,
        block_hash: B256,
        height: u64,
        round: u64,
        stage: RoundStage,
    ) -> Vec<Message> {
        let mut retained = Vec::new();
        let mut drained = Vec::new();

        for message in self.messages.drain(..) {
            if message.block_hash == block_hash
                && message.height == height
                && message.round == round
                && message.stage == stage
            {
                drained.push(message);
            } else {
                retained.push(message);
            }
        }

        self.messages = retained;
        if !drained.is_empty() {
            for _ in 0..drained.len() {
                metrics::increment_counter!(
                    "network_messages_drained_total",
                    "stage" => match stage {
                        RoundStage::Prevote => "prevote",
                        RoundStage::Precommit => "precommit",
                    }
                );
            }
        }
        metrics::gauge!("network_messages_pending_gauge", self.messages.len() as f64);
        drained
    }
}
