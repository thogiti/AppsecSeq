// SPDX-License-Identifier: BUSL-1.1
pragma solidity ^0.8.0;

import {UniConsumer} from "./UniConsumer.sol";
import {IERC20} from "forge-std/interfaces/IERC20.sol";

import {DeltaTracker} from "../types/DeltaTracker.sol";
import {AssetArray, Asset, FEE_SUMMARY_ENTRY_SIZE} from "../types/Asset.sol";
import {AmountA as AmountOut, AmountB as AmountIn} from "../types/Price.sol";
import {SafeTransferLib} from "solady/src/utils/SafeTransferLib.sol";

/// @author philogy <https://github.com/philogy>
/// @dev Handles settlement as well as maintaining Angstrom's solvency invariant.
abstract contract Settlement is UniConsumer {
    using SafeTransferLib for address;

    error BundlDeltaUnresolved(address asset);

    DeltaTracker internal bundleDeltas;

    mapping(address asset => mapping(address owner => uint256 balance)) internal _balances;

    /// @notice Pulls tokens from the caller and credits them to the caller for trading.
    /// @dev WARN: Assumes `asset` charges 0 fees upon transfers and is not rebasing.
    function deposit(address asset, uint256 amount) external {
        asset.safeTransferFrom(msg.sender, address(this), amount);
        _balances[asset][msg.sender] += amount;
    }

    /// @notice Pulls tokens from the caller and credits them to the `to` address for trading.
    /// @dev WARN: Assumes `asset` charges 0 fees upon transfers and is not rebasing.
    function deposit(address asset, address to, uint256 amount) external {
        asset.safeTransferFrom(msg.sender, address(this), amount);
        _balances[asset][to] += amount;
    }

    function withdraw(address asset, uint256 amount) external {
        _balances[asset][msg.sender] -= amount;
        asset.safeTransfer(msg.sender, amount);
    }

    function withdraw(address asset, address to, uint256 amount) external {
        _balances[asset][msg.sender] -= amount;
        asset.safeTransfer(to, amount);
    }

    function _takeAssets(AssetArray assets) internal {
        uint256 length = assets.len();
        for (uint256 i = 0; i < length; i++) {
            _take(assets.getUnchecked(i));
        }
    }

    function _take(Asset asset) internal {
        uint256 amount = asset.take();
        if (amount > 0) {
            address addr = asset.addr();
            UNI_V4.take(_c(addr), address(this), amount);
            bundleDeltas.add(addr, amount);
        }
    }

    function _saveAndSettle(AssetArray assets) internal {
        uint256 length = assets.len();

        // Allocate fee summary buffer.
        uint256 raw_feeSummaryStartPtr;
        assembly ("memory-safe") {
            raw_feeSummaryStartPtr := mload(0x40)
            mstore(0x40, add(raw_feeSummaryStartPtr, mul(length, FEE_SUMMARY_ENTRY_SIZE)))
        }
        uint256 raw_feeSummaryPtr = raw_feeSummaryStartPtr;

        for (uint256 i = 0; i < length; i++) {
            Asset asset = assets.getUnchecked(i);
            address addr = asset.addr();
            uint256 saving = asset.save();
            uint256 settle = asset.settle();

            int256 delta = bundleDeltas.sub(addr, saving + settle);

            if (delta != 0) {
                revert BundlDeltaUnresolved(addr);
            }

            if (settle > 0) {
                UNI_V4.sync(_c(addr));
                addr.safeTransfer(address(UNI_V4), settle);
                UNI_V4.settle();
            }

            asset.raw_copyFeeEntryToMemory(raw_feeSummaryPtr);
            unchecked {
                raw_feeSummaryPtr += FEE_SUMMARY_ENTRY_SIZE;
            }
        }

        // Hash buffer and emit unique log.
        assembly ("memory-safe") {
            mstore(0x00, keccak256(raw_feeSummaryStartPtr, mul(length, FEE_SUMMARY_ENTRY_SIZE)))
            log0(0x00, 0x20)
        }
    }

    function _settleOrderIn(address from, address asset, AmountIn amountIn, bool useInternal)
        internal
    {
        uint256 amount = amountIn.into();

        bundleDeltas.add(asset, amount);
        if (useInternal) {
            _balances[asset][from] -= amount;
        } else {
            asset.safeTransferFrom(from, address(this), amount);
        }
    }

    function _settleOrderOut(address to, address asset, AmountOut amountOut, bool useInternal)
        internal
    {
        uint256 amount = amountOut.into();
        bundleDeltas.sub(asset, amount);
        if (useInternal) {
            _balances[asset][to] += amount;
        } else {
            asset.safeTransfer(to, amount);
        }
    }
}
