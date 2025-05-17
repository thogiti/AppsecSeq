use rand::{Rng, distr::StandardUniform, prelude::Distribution};

use crate::sol_bindings::{
    grouped_orders::{AllOrders, FlashVariants, StandingVariants},
    rpc_orders::{
        ExactFlashOrder, ExactStandingOrder, OrderMeta, PartialFlashOrder, PartialStandingOrder,
        TopOfBlockOrder
    },
    testnet::random::RandomizerSized
};

impl Distribution<AllOrders> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> AllOrders {
        let rand_variant = rng.random_range(0..5);

        match rand_variant {
            0 => AllOrders::TOB(rng.random()),
            1 => AllOrders::ExactStanding(rng.random()),
            2 => AllOrders::PartialStanding(rng.random()),
            3 => AllOrders::PartialFlash(rng.random()),
            4 => AllOrders::ExactFlash(rng.random()),
            _ => unreachable!()
        }
    }
}

impl Distribution<FlashVariants> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> FlashVariants {
        let rand_variant = rng.random_range(0..2);

        match rand_variant {
            0 => FlashVariants::Exact(rng.random()),
            1 => FlashVariants::Partial(rng.random()),
            _ => unreachable!()
        }
    }
}

impl Distribution<ExactFlashOrder> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> ExactFlashOrder {
        ExactFlashOrder { hook_data: rng.gen_sized::<150>(), ..rng.random() }
    }
}

impl Distribution<PartialFlashOrder> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> PartialFlashOrder {
        PartialFlashOrder { hook_data: rng.gen_sized::<150>(), ..rng.random() }
    }
}

impl Distribution<StandingVariants> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> StandingVariants {
        let rand_variant = rng.random_range(0..2);

        match rand_variant {
            0 => StandingVariants::Exact(rng.random()),
            1 => StandingVariants::Partial(rng.random()),
            _ => unreachable!()
        }
    }
}

impl Distribution<ExactStandingOrder> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> ExactStandingOrder {
        ExactStandingOrder { hook_data: rng.gen_sized::<150>(), ..rng.random() }
    }
}

impl Distribution<PartialStandingOrder> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> PartialStandingOrder {
        PartialStandingOrder { hook_data: rng.gen_sized::<150>(), ..rng.random() }
    }
}

impl Distribution<TopOfBlockOrder> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> TopOfBlockOrder {
        TopOfBlockOrder { ..rng.random() }
    }
}

impl Distribution<OrderMeta> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> OrderMeta {
        OrderMeta {
            isEcdsa:   rng.random(),
            from:      rng.random(),
            signature: rng.gen_sized::<64>()
        }
    }
}
