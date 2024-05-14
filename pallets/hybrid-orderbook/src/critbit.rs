
use sp_core::U256;
use core::ops::BitAnd;

use super::*;

#[derive(Encode, Decode, Debug, Default, Clone, PartialEq, TypeInfo)]
pub struct CritbitTree<K, V> {
    /// Index of the root node which is part of the internal nodes.
    root: K,
    /// The internal nodes of the tree.
    internal_nodes: BTreeMap<K, InternalNode<K>>,
    /// The leaf nodes of the tree.
    leaves: BTreeMap<K, LeafNode<K, V>>,
    /// Index of the largest value of the leaf nodes. Could be updated for every insertion.
    max_leaf: K,
    /// Index of the smallest value of the leaf nodes. Could be updated for every insertion.
    min_leaf: K,
    /// Index of the next internal node which should be incremented for every insertion.
    next_internal_node_index: K,
    /// Index of the next leaf node which should be incremented for every insertion.
    next_leaf_node_index: K,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo)]
pub enum NodeKind {
    /// The node is an interior node.
    Internal,
    /// The node is a leaf node.
    #[default]
    Leaf,
}

impl<K, V> CritbitTree<K, V> 
where
    K: CritbitTreeIndex,
    V: Clone,
{   
    /// Create new instance of the tree.
    pub fn new() -> Self {
        Self {
            root: K::PARTITION_INDEX,
            internal_nodes: Default::default(),
            leaves: Default::default(),
            max_leaf: K::PARTITION_INDEX,
            min_leaf: K::PARTITION_INDEX,
            next_internal_node_index: Default::default(),
            next_leaf_node_index: Default::default(),
        }
    }

    /// Check if the leaves are empty.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Get the number of leaf nodes in the tree.
    pub fn size(&self) -> usize {
        self.leaves.len()
    }

    pub fn max_leaf(&self) -> Option<(K, K)> {
        if let Some(leaf) = self.leaves.get(&self.max_leaf) {
            Some((self.max_leaf, leaf.key.clone()))
        } else {
            None
        }
    }

    pub fn min_leaf(&self) -> Option<(K, K)> {
        if let Some(leaf) = self.leaves.get(&self.min_leaf) {
            Some((self.min_leaf, leaf.key.clone()))
        } else {
            None
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<(), CritbitTreeError> {
        let new_leaf = LeafNode::new(key, value);
        let new_leaf_index = self.next_index(NodeKind::Leaf)?;
        if let Some(_) = self.leaves.insert(new_leaf_index, new_leaf) {
            return Err(CritbitTreeError::UniqueIndex);
        }
        let closest_leaf_index = self.get_closet_leaf_index(&key)?;
        if closest_leaf_index == K::PARTITION_INDEX {
            // Handle first insertion
            self.root = K::MAX_INDEX;
            self.max_leaf = new_leaf_index;
            self.min_leaf = new_leaf_index; 
            return Ok(())
        }
        let closest_leaf_key = self.leaves.get(&closest_leaf_index).ok_or(CritbitTreeError::NotFound)?.key;
        if closest_leaf_key == key {
            return Err(CritbitTreeError::AlreadyExist);
        }
        let new_mask = K::new_mask(&key, &closest_leaf_key);
        let new_internal_node = InternalNode::new(new_mask);
        let new_internal_index = self.next_index(NodeKind::Internal)?;
        if let Some(_) = self.internal_nodes.insert(new_internal_index, new_internal_node.clone()) {
            return Err(CritbitTreeError::UniqueIndex);
        }
        let mut curr = self.root;
        let mut internal_node_parent_index = K::PARTITION_INDEX;
        while curr < K::PARTITION_INDEX {
            let internal_node = self.internal_nodes.get(&curr).ok_or(CritbitTreeError::NotFound)?;
            if new_mask > internal_node.mask {
                break;
            }
            internal_node_parent_index = curr;
            if internal_node.mask & key == Zero::zero() {
                curr = internal_node.left;
            } else {
                curr = internal_node.right;
            }
        }
        if internal_node_parent_index == K::PARTITION_INDEX {
            self.root = new_internal_index;
        } else {
            let is_left_child = self.is_left_child(&internal_node_parent_index, &curr)?;
            self.update_ref(internal_node_parent_index, new_internal_index, is_left_child)?;
        }
        let is_left_child = key & new_internal_node.mask == Zero::zero();
        self.update_ref(new_internal_index, new_leaf_index, is_left_child)?;
        self.update_ref(new_internal_index, curr, !is_left_child)?;
        
        Ok(())
    }

    fn is_left_child(&self, parent: &K, index: &K) -> Result<bool, CritbitTreeError> {
        let internal_node = self.internal_nodes.get(parent).ok_or(CritbitTreeError::NotFound)?;
        Ok(internal_node.left == *index)
    }

    fn next_index(&mut self, kind: NodeKind) -> Result<K, CritbitTreeError> {
        match kind {
            NodeKind::Leaf => {
                let index = self.next_leaf_node_index;
                self.next_leaf_node_index += One::one();
                ensure!(self.next_leaf_node_index <= K::CAPACITY, CritbitTreeError::ExceedCapacity);
                Ok(index)
            }
            NodeKind::Internal => {
                let index = self.next_internal_node_index;
                self.next_internal_node_index = self.next_internal_node_index.checked_add(&One::one()).ok_or(CritbitTreeError::Overflow)?;
                Ok(index)
            }
        }
    }

    /// Update the tree reference which could be 'leaf' or 'internal' node.
    fn update_ref(&mut self, parent: K, child: K, is_left_child: bool) -> Result<(), CritbitTreeError> {
        let mut internal_node = self.internal_nodes.get(&parent).ok_or(CritbitTreeError::NotFound)?.clone();
        if is_left_child {
            internal_node.left = child;
        } else {
            internal_node.right = child;
        }
        self.internal_nodes.insert(parent, internal_node);
        if child > K::PARTITION_INDEX {
            let leaf_node_index = K::MAX_INDEX - child;
            let mut leaf_node = self.leaves.get(&leaf_node_index).ok_or(CritbitTreeError::NotFound)?.clone();
            leaf_node.parent = parent;
            self.leaves.insert(leaf_node_index, leaf_node);            
        } else {
            let mut internal_node = self.internal_nodes.get(&child).ok_or(CritbitTreeError::NotFound)?.clone();
            internal_node.parent = parent.clone();
            self.internal_nodes.insert(child, internal_node);
        }
        Ok(())
    }

    fn get_closet_leaf_index(&self, key: &K) -> Result<K, CritbitTreeError> {
        let mut index = self.root;
        if index == K::PARTITION_INDEX {
            return Ok(index);
        }
        while index < K::PARTITION_INDEX {
            let internal_node = self.internal_nodes.get(key).ok_or(CritbitTreeError::NotFound)?;
            if internal_node.mask & *key == Zero::zero() {
                // left
                index = internal_node.left;
            } else {
                // right
                index = internal_node.right;
            }
        }
        Ok(K::MAX_INDEX - index)
    }
}

#[derive(Debug)]
pub enum CritbitTreeError {
    /// The number of leaf nodes exceeds the capacity of the tree.
    ExceedCapacity,
    /// The index overflows the maximum index of the tree.
    Overflow,
    /// The key is not found in the tree.
    NotFound,
    /// The index is already in use.
    UniqueIndex,
    /// The key already exists in the tree.
    AlreadyExist,
}

#[derive(Encode, Decode, Debug, Default, Clone, PartialEq, TypeInfo)]
pub struct InternalNode<K> {
    /// Mask for branching the tree based on the critbit.
    mask: K,
    /// Parent index of the node.
    parent: K,
    /// Left child index of the node.
    left: K,
    /// Right child index of the node.
    right: K,
}

impl<K: CritbitTreeIndex> InternalNode<K> {
    /// Create new instance of the interior node.
    pub fn new(mask: K) -> Self {
        InternalNode {
            mask,
            parent: K::PARTITION_INDEX,
            left: K::PARTITION_INDEX,
            right: K::PARTITION_INDEX,
        }
    }
}

#[derive(Encode, Decode, Debug, Default, Clone, PartialEq, TypeInfo)]
pub struct LeafNode<K, V> {
    /// Parent index of the node.
    parent: K,
    /// Key of the node.
    key: K,
    /// Value of the node.
    value: V
}

impl<K: CritbitTreeIndex, V> LeafNode<K, V> {
    /// Create new instance of the leaf node.
    pub fn new(key: K, value: V) -> Self {
        LeafNode {
            parent: K::PARTITION_INDEX,
            key,
            value
        }
    }
}

/// Index trait for the critbit tree.
pub trait CritbitTreeIndex: Default + AtLeast32BitUnsigned + Copy + BitAnd<Output=Self> {
    /// Maximum index value.
    const MAX_INDEX: Self;
    /// Partition index value. This index is for partitioning between internal and leaf nodes.
    /// Index of the internal nodes is always less than `PARTITION_INDEX`.
    /// While index of the leaf nodes is always greater than or equal to `PARTITION_INDEX`.
    const PARTITION_INDEX: Self;
    /// Maximum number of leaf nodes that can be stored in the tree.
    const CAPACITY: Self;

    /// Calculate new mask. 
    /// First, find the position(pos) of the most significant bit in the XOR of the two indexes.
    /// Then, right shift the mask by that position(e.g. 1 << pos).
    fn new_mask(&self, closest_key: &Self) -> Self;
}

macro_rules! impl_critbit_tree_index {
    ($type: ty, $higher_type: ty) => {
        impl CritbitTreeIndex for $type {
            const MAX_INDEX: Self = <$type>::MAX;
            const PARTITION_INDEX: Self = 1 << (<$type>::BITS - 1);
            const CAPACITY: Self = <$type>::MAX_INDEX - <$type>::PARTITION_INDEX;

            fn new_mask(&self, closest_key: &Self) -> Self {
                let critbit = <$higher_type>::from(self ^ closest_key);
                let pos = <$type>::BITS - (critbit.leading_zeros() - <$type>::BITS);
                1 << (pos-1)
            }
        }
    }
}

impl_critbit_tree_index!(u32, u64);
impl_critbit_tree_index!(u64, u128);
impl_critbit_tree_index!(u128, U256);

mod tests {
    use super::*;

    #[test]
    fn test_critbit_tree() {
        assert_eq!(u64::new_mask(&0,&1).leading_zeros(), 63)
    }

    #[test]
    fn insert_works() {
        let mut tree = CritbitTree::<u64, u64>::new();
        let k = 0x1000u64;
        let v = 0u64;
        tree.insert(k, v).unwrap();
        assert_eq!(tree.size(), 1);
        assert_eq!(tree.root, u64::MAX);
        let k = 0x100u64;
        let v = 0u64;
        if let Err(e) = tree.insert(k, v) {
            println!("Error: {:?}", e);
        }
        assert_eq!(tree.size(), 2);
        assert_eq!(tree.root, 0u64);
        println!("{:?}", tree);
    }
}

