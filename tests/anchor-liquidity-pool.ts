import * as anchor from "@project-serum/anchor";
import {AnchorProvider, Program} from "@project-serum/anchor";
import {AnchorLiquidityPool} from "../target/types/anchor_liquidity_pool";
import NodeWallet from '@project-serum/anchor/dist/cjs/nodewallet';
import {Connection, LAMPORTS_PER_SOL, PublicKey} from "@solana/web3.js";
import {
    Account, approve,
    AuthorityType,
    createAccount,
    createMint,
    getAccount,
    getMint, Mint, mintTo,
    setAuthority,
    TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

// Pool token amount to withdraw / deposit
const POOL_TOKEN_AMOUNT = 10000000;
const DEFAULT_TOKEN_A = 1000000;
const DEFAULT_TOKEN_B = 1000000;
const connection = new Connection("http://localhost:8899", "confirmed");

const options = AnchorProvider.defaultOptions();
const wallet = NodeWallet.local();
const provider = new AnchorProvider(connection, wallet, options);
anchor.setProvider(provider);

const program = anchor.workspace.AnchorLiquidityPool as Program<AnchorLiquidityPool>;

const admin = wallet.payer
const userA = anchor.web3.Keypair.generate();
const userB = anchor.web3.Keypair.generate();

let poolMintPubkey: PublicKey
let aMintPubkey: PublicKey
let bMintPubkey: PublicKey

let poolAccountForAdmin: PublicKey
let poolAccountForUserA: PublicKey
let aAccountForUserA: PublicKey
let bAccountForUserA: PublicKey
let aAccountForUserB: PublicKey
let bAccountForUserB: PublicKey
let aAccountForPDA: PublicKey
let bAccountForPDA: PublicKey
let pda: PublicKey
let aForUserA: Account
let bForUserA: Account
let aForPDA : Account
let bForPDA: Account
let poolForAdmin: Account
let poolForUserA: Account
let poolMint: Mint
const swapPair = anchor.web3.Keypair.generate();

describe("anchor-liquidity-pool", () => {

    it("Setup", async () => {
        await connection.requestAirdrop(userA.publicKey, LAMPORTS_PER_SOL * 10);
        await connection.requestAirdrop(userB.publicKey, LAMPORTS_PER_SOL * 10);

        aMintPubkey = await createMint(connection, admin, admin.publicKey, null, 2, undefined, undefined, TOKEN_PROGRAM_ID);
        bMintPubkey = await createMint(connection, admin, admin.publicKey, null, 2, undefined, undefined, TOKEN_PROGRAM_ID);

        const [_pda, _] = await PublicKey.findProgramAddress([Buffer.from("pool"), aMintPubkey.toBuffer(),bMintPubkey.toBuffer()], program.programId);
        pda = _pda

        poolMintPubkey = await createMint(connection, admin, pda, null, 2, undefined, undefined, TOKEN_PROGRAM_ID);

        console.table([
            {name : "Admin", address: admin.publicKey.toBase58()},
            {name : "UserA", address: userA.publicKey.toBase58()},
            {name : "UserB", address: userB.publicKey.toBase58()},
            {name : "PDA", address: pda.toBase58()}
        ])
        await new Promise((resolve) => setTimeout(resolve, 500));
        poolMint = await getMint(connection, poolMintPubkey, null, TOKEN_PROGRAM_ID)
        const aMint = await getMint(connection, aMintPubkey, null, TOKEN_PROGRAM_ID)
        const bMint = await getMint(connection, bMintPubkey, null, TOKEN_PROGRAM_ID)
        console.log("Token Mint")
        console.table([
            {name : "LP", address: poolMintPubkey.toBase58(), authority: poolMint.mintAuthority.toBase58()},
            {name : "A", address: aMintPubkey.toBase58(), authority: aMint.mintAuthority.toBase58()},
            {name : "B", address: bMintPubkey.toBase58(), authority: bMint.mintAuthority.toBase58()},
        ])

        poolAccountForAdmin = await createAccount(connection, admin, poolMintPubkey, admin.publicKey, undefined, undefined, TOKEN_PROGRAM_ID);
        poolAccountForUserA = await createAccount(connection, admin, poolMintPubkey, userA.publicKey, undefined, undefined, TOKEN_PROGRAM_ID);
        aAccountForUserA = await createAccount(connection, userA, aMintPubkey, userA.publicKey, undefined, undefined, TOKEN_PROGRAM_ID);
        bAccountForUserA = await createAccount(connection, userA, bMintPubkey, userA.publicKey, undefined, undefined, TOKEN_PROGRAM_ID);
        aAccountForUserB = await createAccount(connection, userB, aMintPubkey, userB.publicKey, undefined, undefined, TOKEN_PROGRAM_ID);
        bAccountForUserB = await createAccount(connection, userB, bMintPubkey, userB.publicKey, undefined, undefined, TOKEN_PROGRAM_ID);
        aAccountForPDA = await createAccount(connection, admin, aMintPubkey, admin.publicKey, undefined, undefined, TOKEN_PROGRAM_ID);
        bAccountForPDA = await createAccount(connection, admin, bMintPubkey, admin.publicKey, undefined, undefined, TOKEN_PROGRAM_ID);
        await setAuthority(connection, admin, aAccountForPDA, admin.publicKey, AuthorityType.AccountOwner, pda, undefined, undefined, TOKEN_PROGRAM_ID)
        await setAuthority(connection, admin, bAccountForPDA, admin.publicKey, AuthorityType.AccountOwner, pda, undefined, undefined, TOKEN_PROGRAM_ID)
        await mintTo(connection, admin, aMint.address, aAccountForPDA, aMint.mintAuthority, DEFAULT_TOKEN_A)
        await mintTo(connection, admin, bMint.address, bAccountForPDA, bMint.mintAuthority, DEFAULT_TOKEN_B)
        await mintTo(connection, admin, aMint.address, aAccountForUserA, aMint.mintAuthority, DEFAULT_TOKEN_A)
        await mintTo(connection, admin, bMint.address, bAccountForUserA, bMint.mintAuthority, DEFAULT_TOKEN_B)
        await mintTo(connection, admin, aMint.address, aAccountForUserB, aMint.mintAuthority, DEFAULT_TOKEN_A * 10)
        await mintTo(connection, admin, bMint.address, bAccountForUserB, bMint.mintAuthority, DEFAULT_TOKEN_B * 10)
    })

    it("Create Swap", async () => {
        const fees = {
            tradeFeeNumerator: new anchor.BN(25),
            tradeFeeDenominator: new anchor.BN(10000),
            ownerTradeFeeNumerator: new anchor.BN(5),
            ownerTradeFeeDenominator: new anchor.BN(10000),
            ownerWithdrawFeeNumerator: new anchor.BN(0),
            ownerWithdrawFeeDenominator: new anchor.BN(0),
            hostFeeNumerator: new anchor.BN(20),
            hostFeeDenominator: new anchor.BN(100),
        }
        try {
            const tx = await program.methods.initialize(fees)
                .accounts({
                    pair: swapPair.publicKey,
                    pool: poolMintPubkey,
                    pda: pda,
                    tokenAForPda: aAccountForPDA,
                    tokenBForPda: bAccountForPDA,
                    tokenPoolForInitializer: poolAccountForAdmin,
                    tokenPoolForFeeReceiver: poolAccountForAdmin,
                    tokenProgram: TOKEN_PROGRAM_ID,
                }).preInstructions([
                    await program.account.swapPair.createInstruction(swapPair),
                ]).signers([swapPair]).rpc();
            console.log("Initialize transaction signature", tx);
        }catch (e) {
           console.error(e)
            throw e
        }

        await new Promise((resolve) => setTimeout(resolve, 500));
        aForUserA = await getAccount(connection, aAccountForUserA, null, TOKEN_PROGRAM_ID)
        bForUserA = await getAccount(connection, bAccountForUserA, null, TOKEN_PROGRAM_ID)
        aForPDA = await getAccount(connection, aAccountForPDA, null, TOKEN_PROGRAM_ID)
        bForPDA = await getAccount(connection, bAccountForPDA, null, TOKEN_PROGRAM_ID)
        poolForAdmin = await getAccount(connection, poolAccountForAdmin, null, TOKEN_PROGRAM_ID)
        poolForUserA = await getAccount(connection, poolAccountForUserA, null, TOKEN_PROGRAM_ID)
        console.log("Account")
        console.table([
            {name : "Swap Pair", address: swapPair.publicKey.toBase58(), owner: (await connection.getAccountInfo(swapPair.publicKey)).owner.toBase58()},
        ])

        console.log("Token Account")
        console.table([
            {name : "A for UserA", address: aAccountForUserA.toBase58(), owner: aForUserA.owner.toBase58(), amount: await getTokenBalance(aAccountForUserA)},
            {name : "B for UserA", address: bAccountForUserA.toBase58(), owner: bForUserA.owner.toBase58(), amount: await getTokenBalance(bAccountForUserA)},
            {name : "LP for UserA", address: poolAccountForUserA.toBase58(), owner: poolForUserA.owner.toBase58(), amount: await getTokenBalance(poolAccountForUserA)},
            {name : "A for PDA ", address: aAccountForPDA.toBase58(), owner: aForPDA.owner.toBase58(), amount: await getTokenBalance(aAccountForPDA)},
            {name : "B for PDA ", address: bAccountForPDA.toBase58(), owner: bForPDA.owner.toBase58(), amount:  await getTokenBalance(bAccountForPDA)},
            {name : "LP for Admin", address: poolAccountForAdmin.toBase58(), owner: poolForAdmin.owner.toBase58(), amount: await getTokenBalance(poolAccountForAdmin)},
        ])
    });

    it("Flow", async () => {
       try {
           await depositAll("UserA", POOL_TOKEN_AMOUNT, userA, poolAccountForUserA, aAccountForUserA, bAccountForUserA)
           await depositSingle("UserA", 100000, userA, poolAccountForUserA, aAccountForUserA)
           await withdrawAll("UserA", 100000, userA, poolAccountForUserA, aAccountForUserA, bAccountForUserA)
           await withdrawSingle("UserA", 100000, userA, poolAccountForUserA, aAccountForUserA)
           await swap(2000000, userB,aAccountForUserB, bAccountForUserB)
           await swap(2000000, userB,aAccountForUserB, bAccountForUserB)
           await swap(2000000, userB,aAccountForUserB, bAccountForUserB)
           await swap(2000000, userB,aAccountForUserB, bAccountForUserB)
       }catch(e) {
           console.error(e)
           throw e
       }
    })

});

const getTokenBalance = async (pubkey: PublicKey) => {
    try {
        return parseInt(
            (await connection.getTokenAccountBalance(pubkey)).value.amount
        );
    } catch (e) {
        console.error(`Not a token account ${pubkey}`);
        return NaN;
    }
};

const depositAll = async (name: string, amount: number, user: anchor.web3.Keypair, poolForUser: PublicKey, aForUser: PublicKey, bForUser: PublicKey) => {
    poolMint = await getMint(connection, poolMintPubkey, null, TOKEN_PROGRAM_ID)
    aForPDA = await getAccount(connection, aAccountForPDA, null, TOKEN_PROGRAM_ID)
    bForPDA = await getAccount(connection, bAccountForPDA, null, TOKEN_PROGRAM_ID)
    const maxTokenA = Math.floor((Number(aForPDA.amount)* amount) / Number(poolMint.supply));
    const maxTokenB = Math.floor((Number(bForPDA.amount)* amount) / Number(poolMint.supply));
    const tx = await program.methods.depositAll(new anchor.BN(amount), new anchor.BN(maxTokenA), new anchor.BN(maxTokenB))
        .accounts({
            depositor: user.publicKey,
            pair: swapPair.publicKey,
            pool: poolMintPubkey,
            pda: pda,
            tokenAForPda: aAccountForPDA,
            tokenBForPda: bAccountForPDA,
            tokenAForDepositor: aForUser,
            tokenBForDepositor: bForUser,
            tokenPoolForDepositor: poolForUser,
            tokenProgram: TOKEN_PROGRAM_ID,
        }).signers([user]).rpc()
    console.log("Deposit transaction signature", tx);
    await new Promise((resolve) => setTimeout(resolve, 500));
    console.table([
        {name : `A for ${name}`, address: aForUser.toBase58(), amount: await getTokenBalance(aForUser)},
        {name : `B for ${name}`, address: bForUser.toBase58(), amount: await getTokenBalance(bForUser)},
        {name : `LP for ${name}`, address: poolForUser.toBase58(), amount: await getTokenBalance(poolForUser)},
        {name : "A for PDA ", address: aAccountForPDA.toBase58(), amount: await getTokenBalance(aAccountForPDA)},
        {name : "B for PDA ", address: bAccountForPDA.toBase58(), amount:  await getTokenBalance(bAccountForPDA)},
        {name : "LP for Admin", address: poolAccountForAdmin.toBase58(), amount: await getTokenBalance(poolAccountForAdmin)},
    ])
}

const depositSingle = async (name: string, amount: number, user: anchor.web3.Keypair, poolForUser: PublicKey, sourceForUser: PublicKey) => {
    poolMint = await getMint(connection, poolMintPubkey, null, TOKEN_PROGRAM_ID)
    const tx = await program.methods.depositSingle(new anchor.BN(amount), new anchor.BN(amount / 10))
        .accounts({
            depositor: user.publicKey,
            pair: swapPair.publicKey,
            pool: poolMintPubkey,
            pda: pda,
            tokenAForPda: aAccountForPDA,
            tokenBForPda: bAccountForPDA,
            tokenSourceForDepositor: sourceForUser,
            tokenPoolForDepositor: poolForUser,
            tokenProgram: TOKEN_PROGRAM_ID,
        }).signers([user]).rpc()
    console.log("Deposit Single transaction signature", tx);
    await new Promise((resolve) => setTimeout(resolve, 500));
    console.table([
        {name : `A for ${name}`, address: aAccountForUserA.toBase58(), amount: await getTokenBalance(aAccountForUserA)},
        {name : `B for ${name}`, address: bAccountForUserA.toBase58(), amount: await getTokenBalance(bAccountForUserA)},
        {name : `LP for ${name}`, address: poolForUser.toBase58(), amount: await getTokenBalance(poolForUser)},
        {name : "A for PDA ", address: aAccountForPDA.toBase58(), amount: await getTokenBalance(aAccountForPDA)},
        {name : "B for PDA ", address: bAccountForPDA.toBase58(), amount:  await getTokenBalance(bAccountForPDA)},
        {name : "LP for Admin", address: poolAccountForAdmin.toBase58(), amount: await getTokenBalance(poolAccountForAdmin)},
    ])
}

const withdrawAll = async (name: string, amount: number, user: anchor.web3.Keypair, poolForUser: PublicKey, aForUser: PublicKey, bForUser: PublicKey) => {
    poolMint = await getMint(connection, poolMintPubkey, null, TOKEN_PROGRAM_ID)
    const feeAmount = 0;
    const poolTokenAmount = amount - feeAmount
    const minTokenA = Math.floor((Number(aForPDA.amount)* poolTokenAmount) / Number(poolMint.supply));
    const minTokenB = Math.floor((Number(bForPDA.amount)* poolTokenAmount) / Number(poolMint.supply));
    const tx = await program.methods.withdrawAll(new anchor.BN(amount), new anchor.BN(minTokenA), new anchor.BN(minTokenB))
        .accounts({
            depositor: user.publicKey,
            pair: swapPair.publicKey,
            poolFeeAccount: poolAccountForAdmin,
            pool: poolMintPubkey,
            pda: pda,
            tokenAForPda: aAccountForPDA,
            tokenBForPda: bAccountForPDA,
            tokenAForDepositor: aForUser,
            tokenBForDepositor: bForUser,
            tokenPoolForDepositor: poolForUser,
            tokenProgram: TOKEN_PROGRAM_ID,
        }).signers([user]).rpc()
    console.log("Withdraw transaction signature", tx);
    await new Promise((resolve) => setTimeout(resolve, 500));
    console.table([
        {name : `A for ${name}`, address: aForUser.toBase58(), amount: await getTokenBalance(aForUser)},
        {name : `B for ${name}`, address: bForUser.toBase58(), amount: await getTokenBalance(bForUser)},
        {name : `LP for ${name}`, address: poolForUser.toBase58(), amount: await getTokenBalance(poolForUser)},
        {name : "A for PDA ", address: aAccountForPDA.toBase58(), amount: await getTokenBalance(aAccountForPDA)},
        {name : "B for PDA ", address: bAccountForPDA.toBase58(), amount:  await getTokenBalance(bAccountForPDA)},
        {name : "LP for Admin", address: poolAccountForAdmin.toBase58(), amount: await getTokenBalance(poolAccountForAdmin)},
    ])
}

const withdrawSingle = async (name: string, amount: number,user: anchor.web3.Keypair, poolForUser: PublicKey, destinationForUser: PublicKey) => {
    poolMint = await getMint(connection, poolMintPubkey, null, TOKEN_PROGRAM_ID)
    const tx = await program.methods.withdrawSingle(new anchor.BN(amount), new anchor.BN(POOL_TOKEN_AMOUNT * 10))
        .accounts({
            depositor: user.publicKey,
            pair: swapPair.publicKey,
            poolFeeAccount: poolAccountForAdmin,
            pool: poolMintPubkey,
            pda: pda,
            tokenAForPda: aAccountForPDA,
            tokenBForPda: bAccountForPDA,
            tokenDestinationForDepositor: destinationForUser,
            tokenPoolForDepositor: poolForUser,
            tokenProgram: TOKEN_PROGRAM_ID,
        }).signers([user]).rpc()
    console.log("Withdraw single transaction signature", tx);
    await new Promise((resolve) => setTimeout(resolve, 500));
    console.table([
        {name : `A for ${name}`, address: aAccountForUserA.toBase58(), amount: await getTokenBalance(aAccountForUserA)},
        {name : `B for ${name}`, address: bAccountForUserA.toBase58(), amount: await getTokenBalance(bAccountForUserA)},
        {name : `LP for ${name}`, address: poolForUser.toBase58(), amount: await getTokenBalance(poolForUser)},
        {name : "A for PDA ", address: aAccountForPDA.toBase58(), amount: await getTokenBalance(aAccountForPDA)},
        {name : "B for PDA ", address: bAccountForPDA.toBase58(), amount:  await getTokenBalance(bAccountForPDA)},
        {name : "LP for Admin", address: poolAccountForAdmin.toBase58(), amount: await getTokenBalance(poolAccountForAdmin)},
    ])
}


const swap = async (amountIn: number, swapper: anchor.web3.Keypair, sourceForUser: PublicKey, destinationForUser: PublicKey) => {
    poolMint = await getMint(connection, poolMintPubkey, null, TOKEN_PROGRAM_ID)
    const tx = await program.methods.swap(new anchor.BN(amountIn), new anchor.BN(amountIn * 0.01))
        .accounts({
            swapper: swapper.publicKey,
            pair: swapPair.publicKey,
            poolFeeAccount: poolAccountForAdmin,
            hostFeeAccount: poolAccountForAdmin,
            pool: poolMintPubkey,
            pda: pda,
            tokenSourceForPda: sourceForUser == aAccountForUserB ? aAccountForPDA : bAccountForPDA,
            tokenDestinationForPda: sourceForUser == aAccountForUserB ? bAccountForPDA :aAccountForPDA,
            tokenSourceForSwapper: sourceForUser,
            tokenDestinationForSwapper: destinationForUser,
            tokenProgram: TOKEN_PROGRAM_ID,
        }).signers([swapper]).rpc()
    console.log("Swap transaction signature", tx);
    await new Promise((resolve) => setTimeout(resolve, 500));
    console.table([
        {name : `A for userA`, address: aAccountForUserA.toBase58(), amount: await getTokenBalance(aAccountForUserA)},
        {name : `B for userA`, address: bAccountForUserA.toBase58(), amount: await getTokenBalance(bAccountForUserA)},
        {name : `LP for userA`, address: poolAccountForUserA.toBase58(), amount: await getTokenBalance(poolAccountForUserA)},
        {name : "A for PDA ", address: aAccountForPDA.toBase58(), amount: await getTokenBalance(aAccountForPDA)},
        {name : "B for PDA ", address: bAccountForPDA.toBase58(), amount:  await getTokenBalance(bAccountForPDA)},
        {name : "LP for Admin", address: poolAccountForAdmin.toBase58(), amount: await getTokenBalance(poolAccountForAdmin)},
        {name : `A for swapper`, address: sourceForUser.toBase58(), amount: await getTokenBalance(sourceForUser)},
        {name : `B for swapper`, address: destinationForUser.toBase58(), amount: await getTokenBalance(destinationForUser)},
    ])
}