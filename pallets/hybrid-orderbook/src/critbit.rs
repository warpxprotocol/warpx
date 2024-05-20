
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
    max_leaf_index: K,
    /// Index of the smallest value of the leaf nodes. Could be updated for every insertion.
    min_leaf_index: K,
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
    V: Clone + PartialOrd,
{   
    /// Create new instance of the tree.
    pub fn new() -> Self {
        Self {
            root: K::PARTITION_INDEX,
            internal_nodes: Default::default(),
            leaves: Default::default(),
            max_leaf_index: K::PARTITION_INDEX,
            min_leaf_index: K::PARTITION_INDEX,
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

    /// Query the maximum leaf node. Return **(key, index)**. 
    /// 
    /// Index indicates the index of the leaf node which is encoded as `(K::MAX_INDEX - tree_index)`
    pub fn max_leaf(&self) -> Result<Option<(K, K)>, CritbitTreeError> {
        if self.leaves.is_empty() { return Ok(None) }
        if let Some(leaf) = self.leaves.get(&self.max_leaf_index) {
            Ok(Some((leaf.key.clone(), self.max_leaf_index)))
        } else {
            // For safety, should not reach here unless leaves are empty
            Err(CritbitTreeError::LeafNodeShouldExist)
        }
    }

    /// Query the minimum leaf node. Return **(key, index)**. 
    /// 
    /// Index indicates the index of the leaf node which is encoded as `(K::MAX_INDEX - tree_index)`
    pub fn min_leaf(&self) -> Result<Option<(K, K)>, CritbitTreeError> {
        if self.leaves.is_empty() { return Ok(None) }
        if let Some(leaf) = self.leaves.get(&self.min_leaf_index) {
            Ok(Some((leaf.key.clone(), self.min_leaf_index)))
        } else {
            // For safety, should not reach here unless leaves are empty
            Err(CritbitTreeError::LeafNodeShouldExist)
        }
    }

    /// Insert a new leaf node into the tree for given key `K` and value `V`
    pub fn insert(&mut self, key: K, value: V) -> Result<(), CritbitTreeError> {
        let new_leaf = LeafNode::new(key, value);
        let new_leaf_index = self.next_index(NodeKind::Leaf)?;
        if let Some(_) = self.leaves.insert(new_leaf_index, new_leaf) {
            return Err(CritbitTreeError::UniqueIndex);
        }
        let closest_leaf_index = self.get_closest_leaf_index(&key)?;
        if closest_leaf_index == None {
            // Handle first insertion
            self.root = K::MAX_INDEX;
            self.max_leaf_index = new_leaf_index;
            self.min_leaf_index = new_leaf_index; 
            return Ok(())
        }
        let closest_leaf_key = self.leaves.get(
            &closest_leaf_index
                .expect("Case for `None` is already handled") // TODO: Safe way
            )
            .ok_or(CritbitTreeError::LeafNodeShouldExist)?
            .key;
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
            let internal_node = self.internal_nodes.get(&curr).ok_or(CritbitTreeError::InternalNodeShouldExist)?;
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
            // If the new internal node is the root
            self.root = new_internal_index;
        } else {
            // Update child for the parent internal node
            let is_left_child = self.is_left_child(&internal_node_parent_index, &curr);
            self.update_ref(internal_node_parent_index, new_internal_index, is_left_child)?;
        }
        // Update child for new internal node
        let is_left_child = key & new_internal_node.mask == Zero::zero();
        self.update_ref(new_internal_index, K::MAX_INDEX - new_leaf_index, is_left_child)?;
        self.update_ref(new_internal_index, curr, !is_left_child)?;
        
        // Update min/max leaf
        self.update_min_max_leaf(new_leaf_index, key);
        Ok(())
    }

    /// Remove leaf for given `index`. 
    /// 
    /// **Index here indicates the index of the leaves**
    fn remove_leaf_by_index(&mut self, index: &K) -> Result<(), CritbitTreeError> {
        let leaf_node = self.leaves.get(index).ok_or(CritbitTreeError::NotFound)?;
        if &self.min_leaf_index == index {
            let (_, next_leaf_index) = self.next_leaf(&leaf_node.key)?
                .ok_or(CritbitTreeError::RemoveNotAllowed)?;
            self.min_leaf_index = next_leaf_index;
        }
        if &self.max_leaf_index == index {
            let (_, prev_leaf_index) = self.previous_leaf(&leaf_node.key)?
                .ok_or(CritbitTreeError::RemoveNotAllowed)?;
            self.max_leaf_index = prev_leaf_index;
        }
        Ok(())
    }

    /// Find previous leaf for given key `K`. Return **(key, index)** where `index` indicates leaf index which is encoded as `K::MAX_INDEX - tree_index`
    /// 
    /// **Key indicates the key of the leaf node.**
    fn previous_leaf(&self, key: &K) -> Result<Option<(K, K)>, CritbitTreeError> {
        if let Some(leaf_index) = self.find_leaf(key)? {
            let mut parent_node_index = self.leaves.get(&leaf_index).ok_or(CritbitTreeError::LeafNodeShouldExist)?.parent;
            let mut ptr = K::MAX_INDEX - leaf_index; 
            while parent_node_index != K::PARTITION_INDEX && self.is_left_child(&parent_node_index, &ptr) {
                ptr = parent_node_index;
                parent_node_index = self.internal_nodes.get(&ptr).ok_or(CritbitTreeError::InternalNodeShouldExist)?.parent;
            }
            // This means previous leaf doesn't exist
            if parent_node_index == K::PARTITION_INDEX {
                return Ok(None)
            }
            let start_index = self.internal_nodes
                .get(&parent_node_index)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?
                .left;
            let leaf_index = K::MAX_INDEX - self.right_most_leaf(start_index)?;
            let leaf_node = self.leaves.get(&leaf_index).ok_or(CritbitTreeError::LeafNodeShouldExist)?;
            
            Ok(Some((leaf_node.clone().key, leaf_index)))
        } else {
            // 1. Empty tree
            // 2. Leaf node for given key doesn't exist
            Ok(None)
        }
    }

    /// Find next leaf for given key `K`. Return **(key, index)** where `index` indicates leaf index which is encoded as `K::MAX_INDEX - tree_index`
    /// 
    /// **Key indicates the key of the leaf node.**
    fn next_leaf(&self, key: &K) -> Result<Option<(K, K)>, CritbitTreeError> {
        if let Some(leaf_index) = self.find_leaf(key)? {
            let mut parent_node_index = self.leaves.get(&leaf_index).ok_or(CritbitTreeError::LeafNodeShouldExist)?.parent;
            let mut ptr = K::MAX_INDEX - leaf_index; 
            while parent_node_index != K::PARTITION_INDEX && !self.is_left_child(&parent_node_index, &ptr) {
                ptr = parent_node_index;
                parent_node_index = self.internal_nodes.get(&ptr).ok_or(CritbitTreeError::InternalNodeShouldExist)?.parent;
            }
            // This means previous leaf doesn't exist
            if parent_node_index == K::PARTITION_INDEX {
                return Ok(None)
            }
            let start_index = self.internal_nodes
                .get(&parent_node_index)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?
                .right;
            let leaf_index = K::MAX_INDEX - self.left_most_leaf(&start_index)?;
            let leaf_node = self.leaves.get(&leaf_index).ok_or(CritbitTreeError::LeafNodeShouldExist)?;
            
            Ok(Some((leaf_node.clone().key, leaf_index)))
        } else {
            // 1. Empty tree
            // 2. Leaf node for given key doesn't exist
            Ok(None)
        }
    }

    /// Get the left most leaf index of the tree. Index indicates the index inside the tree.
    /// 
    /// Should not return `Error` unless the tree is not empty.
    fn left_most_leaf(&self, root: &K) -> Result<K, CritbitTreeError> {
        let mut curr = root.clone();
        while curr < K::PARTITION_INDEX {
            let internal_node = self.internal_nodes.get(&curr).ok_or(CritbitTreeError::InternalNodeShouldExist)?;
            curr = internal_node.left;
        }
        Ok(curr)
    }

    /// Get the right most leaf index of the tree. Index indicates the index inside the tree.
    /// 
    /// Should not return `Error` unless the tree is not empty.
    fn right_most_leaf(&self, root: K) -> Result<K, CritbitTreeError> {
        let mut curr = root.clone();
        while curr < K::PARTITION_INDEX {
            let internal_node = self.internal_nodes.get(&curr).ok_or(CritbitTreeError::InternalNodeShouldExist)?;
            curr = internal_node.right;
        }
        Ok(curr)
    }

    /// Find leaf index for given `key`. 
    /// Return 
    /// 
    /// - `leaf_index` which indicates the index inside the leaf which is encoded as `K::MAX_INDEX - leaf_index`.
    /// - `K::PARTITION_INDEX`, if tree is empty
    /// - `None`, if key doesn't match with closest key
    fn find_leaf(&self, key: &K) -> Result<Option<K>, CritbitTreeError> {
        if let Some(leaf_index) = self.get_closest_leaf_index(key)? {
            let leaf_node = self.leaves.get(&leaf_index).ok_or(CritbitTreeError::LeafNodeShouldExist)?;
            if &leaf_node.key != key {
                Ok(None)
            } else {
                Ok(Some(leaf_index))
            }
        } else {
            // Tree is empty
            Ok(None)
        }
    }

    /// Update the minimum and maximum leaf nodes.
    fn update_min_max_leaf(&mut self, new_leaf_index: K, new_key: K) {
        if let Some(min_leaf_value) = self.leaves.get(&self.min_leaf_index) {
            if min_leaf_value.key > new_key {
                self.min_leaf_index = new_leaf_index;
            }
        } else {
            self.min_leaf_index = new_leaf_index;
        }
        if let Some(max_leaf_value) = self.leaves.get(&self.max_leaf_index) {
            if max_leaf_value.key < new_key {
                self.max_leaf_index = new_leaf_index;
            }
        } else {
            self.max_leaf_index = new_leaf_index;
        }
    }

    /// Check if the index is the left child of the parent.
    fn is_left_child(&self, parent: &K, child: &K) -> bool {
        if let Some(internal_node) = self.internal_nodes.get(parent) {
            return &internal_node.left == child
        }
        // Should not reach here
        false
    }

    /// Get the next index based on `NodeKind`, which maybe leaf or internal for the tree.
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
        let mut internal_node = self.internal_nodes.get(&parent).ok_or(CritbitTreeError::InternalNodeShouldExist)?.clone();
        if is_left_child {
            internal_node.left = child;
        } else {
            internal_node.right = child;
        }
        self.internal_nodes.insert(parent, internal_node);
        if child > K::PARTITION_INDEX {
            let leaf_node_index = K::MAX_INDEX - child;
            let mut leaf_node = self.leaves.get(&leaf_node_index).ok_or(CritbitTreeError::LeafNodeShouldExist)?.clone();
            leaf_node.parent = parent;
            self.leaves.insert(leaf_node_index, leaf_node);            
        } else {
            let mut internal_node = self.internal_nodes.get(&child).ok_or(CritbitTreeError::InternalNodeShouldExist)?.clone();
            internal_node.parent = parent.clone();
            self.internal_nodes.insert(child, internal_node);
        }
        Ok(())
    }

    /// Get the closest leaf index which encoded as `K::MAX_INDEX - tree_index` to the given key.
    fn get_closest_leaf_index(&self, key: &K) -> Result<Option<K>, CritbitTreeError> {
        let mut curr = self.root;
        if curr == K::PARTITION_INDEX {
            // Case: Tree is empty(e.g first insertion)
            return Ok(None);
        }
        while curr < K::PARTITION_INDEX {
            let internal_node = self.internal_nodes
                .get(&curr)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?;
            if internal_node.mask & *key == Zero::zero() {
                curr = internal_node.left;
            } else {
                curr = internal_node.right;
            }
        }

        Ok(Some(K::MAX_INDEX - curr))
    }
}

#[derive(Debug, PartialEq)]
pub enum CritbitTreeError {
    /// The number of leaf nodes exceeds the capacity of the tree.
    ExceedCapacity,
    /// The index overflows the maximum index of the tree.
    Overflow,
    /// The index is already in use.
    UniqueIndex,
    /// The key already exists in the tree.
    AlreadyExist,
    /// Error for safe code
    InternalNodeShouldExist,
    /// Error for safe code
    LeafNodeShouldExist,
    /// `Leaf` or `Internal Node` may not exist for given index
    NotFound,
    /// Error on remove which may be caused by empty tree after remove
    RemoveNotAllowed
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
pub trait CritbitTreeIndex: sp_std::fmt::Debug + Default + AtLeast32BitUnsigned + Copy + BitAnd<Output=Self> {
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

        // Check whether the tree is empty
        assert_eq!(tree.min_leaf().unwrap(), None);
        assert_eq!(tree.max_leaf().unwrap(), None);
        assert!(tree.is_empty());
        assert_eq!(tree.size(), 0);

        // First insertion
        let k1 = 0x1000u64;
        let v = 0u64;
        tree.insert(k1, v).unwrap();
        
        // Check validity of first insertion of tree
        assert_eq!(tree.size(), 1);
        assert_eq!(tree.root, u64::MAX);
        assert_eq!(tree.min_leaf().unwrap(), Some((k1, 0)));
        assert_eq!(tree.max_leaf().unwrap(), Some((k1, 0)));
        
        let previous_leaf = tree.previous_leaf(&k1).unwrap();
        let next_leaf = tree.next_leaf(&k1).unwrap();
        assert_eq!(previous_leaf, None);
        assert_eq!(next_leaf, None);

        assert_eq!(tree.remove_leaf_by_index(&0), Err(CritbitTreeError::RemoveNotAllowed));

        println!("First Insertion => {:?}", tree);

        // Second insertion
        let k2 = 0x100u64;
        let v = 0u64;
        tree.insert(k2, v).unwrap();
        assert_eq!(tree.size(), 2);

        // Check whether root index has updated
        assert_eq!(tree.root, 0u64);
        
        // Check min & max upated 
        assert_eq!(tree.min_leaf().unwrap(), Some((k2, 1)));
        assert_eq!(tree.max_leaf().unwrap(), Some((k1, 0)));

        // Check whether `find_leaf` works
        let leaf_index = tree.find_leaf(&k2);
        assert_eq!(leaf_index.unwrap(), Some(1));

        assert_eq!(tree.previous_leaf(&k2).unwrap(), None);
        assert_eq!(tree.previous_leaf(&k1).unwrap(), Some((k2, 1)));
        assert_eq!(tree.next_leaf(&k2).unwrap(), Some((k1, 0)));

        println!("{:?}", tree);

        assert_eq!(tree.remove_leaf_by_index(&0), Ok(()));
    }
}

