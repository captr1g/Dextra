struct PoolInfo {
        IERC20 depositToken;
        IERC20 rewardToken;
        uint256 minimumDeposit;
        uint256 lockPeriod;
        bool canSwap;
        uint256 lastRate;
        uint256 lastAPY;
    }
PoolInfo[] public poolInfo;
constructor() Ownable(msg.sender) {} 
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

    

