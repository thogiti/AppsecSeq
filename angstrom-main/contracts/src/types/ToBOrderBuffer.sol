// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

struct ToBOrderBuffer {
    bytes32 typeHash;
    uint256 quantityIn;
    uint256 quantityOut;
    uint256 maxGasAsset0;
    bool useInternal;
    address assetIn;
    address assetOut;
    address recipient;
    uint64 validForBlock;
}

using ToBOrderBufferLib for ToBOrderBuffer global;

/// @author philogy <https://github.com/philogy>
library ToBOrderBufferLib {
    uint256 internal constant BUFFER_BYTES = 288;

    /// forgefmt: disable-next-item
    bytes32 internal constant TOP_OF_BLOCK_ORDER_TYPEHASH = keccak256(
        "TopOfBlockOrder("
           "uint128 quantity_in,"
           "uint128 quantity_out,"
           "uint128 max_gas_asset0,"
           "bool use_internal,"
           "address asset_in,"
           "address asset_out,"
           "address recipient,"
           "uint64 valid_for_block"
        ")"
    );

    function init(ToBOrderBuffer memory self) internal view {
        self.typeHash = TOP_OF_BLOCK_ORDER_TYPEHASH;
        self.validForBlock = uint64(block.number);
    }

    function hash(ToBOrderBuffer memory self) internal pure returns (bytes32 orderHash) {
        assembly ("memory-safe") {
            orderHash := keccak256(self, BUFFER_BYTES)
        }
    }
}
